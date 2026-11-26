#!/usr/bin/env python3
"""Real MJH + tmux + local Ansible: background ack and foreground input/return."""
import argparse,json,os,shlex,socket,subprocess,tempfile,time
from pathlib import Path

def wait(check,timeout=25):
 end=time.monotonic()+timeout
 while time.monotonic()<end:
  try:
   v=check()
   if v:return v
  except (OSError,ConnectionError,KeyError):pass
  time.sleep(.08)
 raise AssertionError('Zeitlimit')

def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--tmux',type=Path,required=True);p.add_argument('--ansible-bin',type=Path,required=True);a=p.parse_args()
 with tempfile.TemporaryDirectory(prefix='mjh-docker-input-') as tmp:
  b=Path(tmp);project=b/'project';(project/'bin/control.d').mkdir(parents=True);(project/'.venv/bin').mkdir(parents=True)
  for name in ['ansible-config','ansible-playbook','ansible-inventory']:(project/'.venv/bin'/name).symlink_to(a.ansible_bin/name)
  (project/'ansible.cfg').write_text('[defaults]\nlocal_tmp='+str(b/'local-tmp')+'\nstrategy=linear\n')
  (project/'summary.txt').write_text('LOCAL_DOCKER_SUMMARY')
  (project/'play.yml').write_text('''- hosts: localhost
  gather_facts: false
  vars:
    log_file: "'''+str(project/'summary.txt')+'''"
  tasks:
    - name: Display deployment results
      tags: [controller_docker_summary_ack]
      ansible.builtin.pause:
        prompt: |
          Deployment summary (log contents):
          {{ lookup('file', log_file) }}
          Press Enter to continue.
    - name: Echte Auswahl
      ansible.builtin.pause:
        prompt: "MJH_REAL_CHOICE alpha,beta eingeben"
      register: choice
    - ansible.builtin.assert:
        that: "choice.user_input == 'alpha,beta'"
    - ansible.builtin.pause:
        seconds: 2
''')
  (project/'bin/control').write_text('#!/bin/bash\nexec '+shlex.quote(str(a.ansible_bin/'ansible-playbook'))+' -i localhost, '+shlex.quote(str(project/'play.yml'))+'\n')
  (project/'bin/control.d/docker-deploy-all.json').write_text(json.dumps({'commands':[{'name':'docker-deploy-all','description':'Neutraler Eingabetest','handler':'bin/control'}]}))
  state=b/'state';cfg=b/'config.json';sock=b/'tmux.sock'
  cfg.write_text(json.dumps({'project':str(project),'state':str(state),'tmux_socket':str(sock),'tmux':str(a.tmux),'refresh_seconds':5}))
  def cli(*args):
   result=subprocess.run([str(a.binary),'--config',str(cfg),*args],capture_output=True,text=True)
   assert result.returncode==0,result.stderr
   return result
  def mux(*args):return subprocess.check_output([str(a.tmux),'-S',str(sock),*args],text=True)
  def api(method,**fields):
   with socket.socket(socket.AF_UNIX) as s:
    s.settimeout(15);s.connect(str(state/'api.sock'));s.sendall((json.dumps({'method':method,**fields})+'\n').encode());v=json.loads(s.makefile('rb').readline())
   assert v['ok'],v
   return v['data']
  daemon=None
  try:
   cli('session');log=(b/'daemon.log').open('a');daemon=subprocess.Popen([str(a.binary),'--config',str(cfg),'serve'],stdout=log,stderr=log)
   wait(lambda:(state/'api.sock').exists())
   command=shlex.join([str(a.binary),'--config',str(cfg),'ui'])
   ui=mux('new-window','-P','-F','#{pane_id}','-n','Verwaltung',command).strip()
   wait(lambda:'ANSIBLE TUI' in mux('capture-pane','-p','-t',ui))
   job=api('control.start',command='docker-deploy-all',args=[])
   wait(lambda:'MJH_REAL_CHOICE' in mux('capture-pane','-p','-t',job['pane']))
   wait(lambda:mux('display-message','-p','-t','ansible-tui','#{pane_id}').strip()==job['pane'])
   assert api('jobs.list')[0]['needs_input'] is True
   assert 'LOCAL_DOCKER_SUMMARY' in mux('capture-pane','-p','-S','-','-t',job['pane'])
   mux('send-keys','-t',job['pane'],'alpha,beta','Enter')
   wait(lambda:mux('display-message','-p','-t','ansible-tui','#{pane_id}').strip()==ui)
   wait(lambda:any(j['id']==job['id'] and j['code']==0 for j in api('jobs.list')))
   print('PASS: echtem Ansible Zusammenfassung ohne Enter, echte Auswahl automatisch im Vordergrund, nach Eingabe Verwaltung wieder aktiv, Exit0')
  finally:
   if daemon and daemon.poll() is None:daemon.terminate();daemon.wait(timeout=5)
   subprocess.run([str(a.tmux),'-S',str(sock),'kill-server'],capture_output=True)
if __name__=='__main__':main()
