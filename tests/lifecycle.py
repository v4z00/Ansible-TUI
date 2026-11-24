#!/usr/bin/env python3
"""Native PTYs: Ctrl+Q returns to caller despite dead Shell; persistent ack/clear."""
import errno
import argparse,json,os,pty,select,shlex,signal,socket,sqlite3,subprocess,tempfile,time
from pathlib import Path

def wait(fn,timeout=15):
 end=time.monotonic()+timeout
 while time.monotonic()<end:
  try:
   v=fn()
   if v:return v
  except (OSError,ConnectionError,KeyError):pass
  time.sleep(.05)
 raise AssertionError('Zeitlimit')

def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--tmux',type=Path,required=True);a=p.parse_args();binary=a.binary.resolve();tmux=str(a.tmux.resolve())
 with tempfile.TemporaryDirectory(prefix='mjh-lifecycle-') as tmp:
  b=Path(tmp);project=b/'project';(project/'bin/control.d').mkdir(parents=True)
  (project/'bin/control').write_text('#!/bin/sh\ncase "$1" in\nfail) echo FAILED; exit 7;;\nslow) while true; do echo HEARTBEAT; sleep 0.1; done;;\n*) exit 0;;\nesac\n')
  for name in ['fail','slow','ok']:(project/f'bin/control.d/{name}.json').write_text(json.dumps({'schema':1,'commands':[{'name':name,'description':name,'handler':'bin/control'}]}))
  state=b/'state';state.mkdir();sock=b/'tmux|test.sock';cfg=b/'config.json';cfg.write_text(json.dumps({'project':str(project),'state':str(state),'tmux_socket':str(sock),'tmux':tmux,'max_running':3,'refresh_seconds':60}))
  cmd=[str(binary),'--config',str(cfg)]
  def mux(*args):return subprocess.check_output([tmux,'-S',str(sock),*args],text=True)
  def api(method,**kw):
   with socket.socket(socket.AF_UNIX) as client:
    client.settimeout(10);client.connect(str(state/'api.sock'));client.sendall((json.dumps({'method':method,**kw})+'\n').encode());v=json.loads(client.makefile('rb').readline())
    if not v['ok']:raise RuntimeError(v['error'])
    return v['data']
  def rejected(method,**kw):
   try:api(method,**kw)
   except RuntimeError:return
   raise AssertionError('Aktion faelschlich zugelassen: '+method)
  daemon=None;children=[]
  def client(command):
   pid,fd=pty.fork()
   if pid==0:
    os.environ['TERM']='xterm-256color'
    os.execv('/bin/sh',['sh','-c',command])
   children.append((pid,fd));return pid,fd
  try:
   no_tty=subprocess.run([*cmd,'open'],stdin=subprocess.DEVNULL,capture_output=True)
   assert no_tty.returncode and b'interaktives Terminal' in no_tty.stderr and not sock.exists()
   subprocess.run([*cmd,'session'],check=True)
   log=(b/'service.log').open('w');daemon=subprocess.Popen([*cmd,'serve'],stdout=log,stderr=log)
   wait(lambda:(state/'api.sock').exists());wait(lambda:len(api('catalog.list'))==3)
   failed=api('control.start',command='fail',args=[]);id=failed['id'];wait(lambda:any(j['id']==id and j['state']=='failed' for j in api('jobs.list')))
   rejected('job.ack',id=id,confirm='wrong')
   api('job.ack',id=id,confirm=id);api('job.ack',id=id,confirm=id)
   j=next(j for j in api('jobs.list') if j['id']==id);assert j['state']=='failed' and j['code']==7 and j['acknowledged_at']
   assert len([e for e in api('events.list') if e['action']=='job.acknowledged'])==1
   # A stale sink with buffered output cannot recreate a cleared log.
   path=state/'jobs'/f'{id}.log';sink=subprocess.Popen([*cmd,'log-sink',str(path)],stdin=subprocess.PIPE)
   sink.stdin.write(b'OLD_CAPTURE\n');sink.stdin.flush();wait(lambda:path.exists() and b'OLD_CAPTURE' in path.read_bytes())
   for i in range(1,5):path.with_suffix(f'.log.{i}').write_text('OLD_ROTATION')
   rejected('job.log.clear',id=id,confirm='wrong')
   api('job.log.clear',id=id,confirm=id)
   sink.stdin.write(b'STALE_BUFFERED_OUTPUT\n');sink.stdin.flush();sink.stdin.close();assert sink.wait(timeout=5)==0
   assert api('job.log.read',id=id)['output']==''
   assert all(not path.with_suffix(f'.log.{i}').exists() for i in range(1,5))
   assert not path.exists()
   api('job.close',id=id,confirm=id)
   assert any(j['id']==id and j['code']==7 for j in api('jobs.list'))
   success=api('control.start',command='ok',args=[]);wait(lambda:any(j['id']==success['id'] and j['state']=='finished' for j in api('jobs.list')))
   rejected('job.ack',id=success['id'],confirm=success['id'])
   slow=api('control.start',command='slow',args=[])
   for method in ['job.ack','job.close','job.log.clear']:rejected(method,id=slow['id'],confirm=slow['id'])
   # The exact reported condition: Shell is dead and another job is still live.
   mux('new-window','-d','-n','Shell',"/bin/sh");mux('set-window-option','-t','ansible-tui:Shell','remain-on-exit','on');mux('send-keys','-t','ansible-tui:Shell','exit','Enter');wait(lambda:mux('display-message','-p','-t','ansible-tui:Shell','#{pane_dead}').strip()=='1')
   other_session='unrelated| test'
   mux('new-session','-d','-s',other_session,'sleep 60')
   other,otherfd=client(shlex.join([tmux,'-S',str(sock),'attach-session','-t',other_session]))
   wait(lambda:other_session in mux('list-clients','-F','#{session_name}'))
   pid,fd=client(shlex.join([*cmd,'open'])+"; printf '\\nMJH_RETURNED_TO_CALLER\\n'")
   wait(lambda:'Verwaltung' in mux('list-windows','-t','ansible-tui','-F','#{window_name}'))
   ui=wait(lambda:next((line.split('|',1)[0] for line in mux('list-panes','-s','-t','ansible-tui','-F','#{pane_id}|#{window_name}').splitlines() if line.endswith('|Verwaltung')),None))
   wait(lambda:'ANSIBLE TUI' in mux('capture-pane','-p','-t',ui))
   mux('send-keys','-t',ui,'5');time.sleep(.2)
   # Exit through the attached client, rather than injecting a direct process kill.
   os.write(fd,b'\x11')
   output=b''
   def returned():
    nonlocal output
    if select.select([fd],[],[],.05)[0]:
     try:output+=os.read(fd,65536)
     except OSError:pass
    return b'MJH_RETURNED_TO_CALLER' in output
   wait(returned)
   wait(lambda:'Verwaltung' not in mux('list-windows','-t','ansible-tui','-F','#{window_name}'))
   assert mux('display-message','-p','-t',slow['pane'],'#{pane_dead}').strip()=='0'
   time.sleep(2.2)  # Let the service reconcile the detached but running pane.
   assert next(j for j in api('jobs.list') if j['id']==slow['id'])['state']=='running'
   assert other_session in mux('list-clients','-F','#{session_name}')
   listed={row['name']:row for row in api('tmux.list')['servers'][0]['sessions']}
   assert listed['ansible-tui']['clients']==0 and listed['ansible-tui']['socket']==str(sock)
   assert listed[other_session]['clients']==1 and listed[other_session]['socket']==str(sock)
   rejected('events.clear',confirm='wrong');api('events.clear',confirm='clear-events')
   assert [e['action'] for e in api('events.list')]==['events.cleared']
   daemon.terminate();daemon.wait(timeout=5)
   with sqlite3.connect(state/'catalog.sqlite3') as db:
    db.execute("UPDATE jobs SET state='unknown' WHERE id=?",(slow['id'],))
   daemon=subprocess.Popen([*cmd,'serve'],stdout=log,stderr=log)
   wait(lambda:api('status'));j=next(j for j in api('jobs.list') if j['id']==id)
   assert j['acknowledged_at'] and j['state']=='failed' and j['code']==7
   assert mux('display-message','-p','-t',slow['pane'],'#{pane_dead}').strip()=='0'
   wait(lambda:any(j['id']==slow['id'] and j['state']=='running' for j in api('jobs.list')))
   api('job.stop',id=slow['id'],confirm=slow['id'])
   wait(lambda:any(j['id']==slow['id'] and j['state'] in ['failed','finished'] for j in api('jobs.list')))
   wait(lambda:'ansible-tui' not in mux('list-sessions','-F','#{session_name}').splitlines())
   assert other_session in mux('list-clients','-F','#{session_name}')
   # Missing MJH session is reported, and opening a new UI recreates it.
   pid2,fd2=client(shlex.join([*cmd,'open'])+"; printf '\\nMJH_REOPEN_RETURNED\\n'")
   wait(lambda:any(row['name']=='ansible-tui' for row in api('tmux.list')['servers'][0]['sessions']))
   wait(lambda:'Verwaltung' in mux('list-windows','-t','ansible-tui','-F','#{window_name}'))
   ui2=wait(lambda:next((line.split('|',1)[0] for line in mux('list-panes','-s','-t','ansible-tui','-F','#{pane_id}|#{window_name}').splitlines() if line.endswith('|Verwaltung')),None))
   wait(lambda:'ANSIBLE TUI' in mux('capture-pane','-p','-t',ui2))
   count=len(api('jobs.list'))
   mux('send-keys','-t',ui2,'2','f','C-t');wait(lambda:'Auftrag pruefen' in mux('capture-pane','-p','-t',ui2))
   mux('send-keys','-t',ui2,'Tab','Enter');wait(lambda:len(api('jobs.list'))==count+1)
   wait(lambda:api('jobs.list')[0]['state']=='failed')
   assert mux('display-message','-p','-t','ansible-tui','#{pane_id}').strip()==ui2
   mux('send-keys','-t',ui2,'s','C-t');wait(lambda:'Auftrag pruefen' in mux('capture-pane','-p','-t',ui2))
   mux('send-keys','-t',ui2,'Tab','Tab','Enter');wait(lambda:len(api('jobs.list'))==count+2)
   foreground=wait(lambda:(row if (row:=api('jobs.list')[0])['pane'] else None))
   wait(lambda:mux('display-message','-p','-t','ansible-tui','#{pane_id}').strip()==foreground['pane'])
   assert foreground['label']=='Ansible slow'
   sidebar=mux('show-options','-v','-t','ansible-tui','status-right').strip()
   assert 'Zur Verwaltung: Ctrl+B dann w' in mux('display-message','-p','-t',foreground['pane'],sidebar)
   assert not mux('display-message','-p','-t',ui2,sidebar).strip()
   mux('select-window','-t',ui2);os.write(fd2,b'\x11')
   wait(lambda:'Verwaltung' not in mux('list-windows','-t','ansible-tui','-F','#{window_name}'))
   assert mux('display-message','-p','-t',foreground['pane'],'#{pane_dead}').strip()=='0'
   api('job.stop',id=foreground['id'],confirm=foreground['id'])
   wait(lambda:'ansible-tui' not in mux('list-sessions','-F','#{session_name}').splitlines())
   print('PASS: Startmodus per echter TUI-Vorschau/Tab/Enter: Hintergrund belaesst UI, Vordergrund oeffnet den neuen Auftrag; danach automatisch aufgeraeumt')
   print('PASS: Tmuxliste zeigt alle Sitzungen am Ansible-TUI-Socket; ohne Arbeit MJH automatisch beendet, fremde Sitzung/Client erhalten; erneutes mjh open legt Verwaltung neu an')
   print('PASS: echte Ctrl+Q-Eingabe kehrt trotz toter Shell zum Aufrufer zurueck; UI-Fenster geschlossen; laufender Job und fremder Client erhalten')
   print('PASS: Kenntnisnahme persistent/idempotent, Exit7/Fehler unveraendert; laufende/erfolgreiche Jobs abgewiesen; Verlaufleeren erhaelt Jobs/Quittierung')
   print('PASS: Ausgabelog/Rotationen geleert, alter gepufferter Sink fuellt nichts nach; beendetes Terminal gezielt geschlossen; laufender Auftrag geschuetzt')
  finally:
   if daemon and daemon.poll() is None:daemon.terminate();daemon.wait(timeout=5)
   subprocess.run([tmux,'-S',str(sock),'kill-server'],capture_output=True)
   for pid,fd in children:
    # Drain pending tmux output before closing the PTY. On macOS the child
    # can otherwise block during terminal teardown while waitpid waits for it.
    os.set_blocking(fd, False)
    try:
     while os.read(fd, 65536):
      pass
    except BlockingIOError:
     pass
    except OSError as error:
     if error.errno != errno.EIO:
      raise
    try:os.kill(pid,signal.SIGTERM)
    except ProcessLookupError:pass
    os.close(fd)
    try:os.waitpid(pid,0)
    except ChildProcessError:pass
if __name__=='__main__':main()
