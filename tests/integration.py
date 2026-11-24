#!/usr/bin/env python3
"""Real Unix-socket/SQLite/tmux tests, no real Ansible or SSH targets."""
import argparse
import json
import os
import pty
import signal
import socket
import subprocess
import tempfile
import time
import sys
from pathlib import Path


def wait_for(check, timeout=12):
    end=time.monotonic()+timeout
    while time.monotonic()<end:
        try:
            value=check()
        except (ConnectionError,FileNotFoundError):
            value=None
        if value:
            return value
        time.sleep(.1)
    raise AssertionError('Zeitlimit')


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--binary',type=Path,required=True)
    ap.add_argument('--tmux',required=True)
    args=ap.parse_args()
    binary=args.binary.resolve()
    expected_version=subprocess.check_output([str(binary),'--version'],text=True).strip().split()[-1]
    with tempfile.TemporaryDirectory(prefix='mjh-it-',dir='/tmp') as tmp:
        base=Path(tmp)
        project=base/'ansible'; (project/'bin/control.d').mkdir(parents=True)
        (project/'bin/control').write_text('''#!/usr/bin/env bash
set -eu
name=$1; shift
case "$name" in
base) exec python3 -c 'import json,sys;open("base-argv.json","w").write(json.dumps(sys.argv[1:]))' "$@" ;;
demo) exec python3 -u "$CONTROL_PROJECT_ROOT/bin/demo.py" "$@" ;;
slow|other) echo START; for i in $(seq 1 80); do echo heartbeat; sleep 0.2; done ;;
esac
''')
        (project/'bin/demo.py').write_text('''import json,os,sys
assert sys.stdin.isatty() and sys.stdout.isatty()
with open('/dev/tty') as t: assert t.isatty()
open('argv.json','w').write(json.dumps(sys.argv[1:]))
print('TTY_OK',flush=True)
assert input('Auswahl: ')=='probe'
import getpass
assert getpass.getpass('Passwort: ')=='dummy-hidden'
print('DONE',flush=True)
sys.exit(7)
''')
        for name in ['base','demo','docker-deploy','slow','other']:
            (project/f'bin/control.d/{name}.json').write_text(json.dumps({'schema':1,'commands':[{'name':name,'description':name,'handler':'bin/control'}]}))
        docs=project/'descriptions/commands';docs.mkdir(parents=True)
        metadata={'schema_version':1,'kind':'command','ref':'demo','documented':True,'fields':[{'id':'mode','label':'Modus','description':'Neutraler Testparameter','type':'string','choices':['eins','zwei'],'binding':{'kind':'option','name':'--mode'},'sensitive':False},{'id':'retries','label':'Wiederholungen','type':'integer','default':2,'examples':[2,5],'binding':{'kind':'option','name':'--retries'}}]}
        (docs/'demo.md').write_text('# Demo-Anleitung\n\n```json\n'+json.dumps(metadata)+'\n```\n')
        base_meta={'schema_version':1,'kind':'command','ref':'base','documented':True,'fields':[
            {'id':'targets','label':'targets','type':'array','binding':{'kind':'option','name':'--limit','separator':','},'completion':{'source':'inventory.targets'}},
            {'id':'all','label':'all','type':'boolean','binding':{'kind':'flag','name':'--all'}}
        ],'constraints':[{'kind':'exactly_one_of','fields':['targets','all']}]}
        (docs/'base.md').write_text('# Base\n```json\n'+json.dumps(base_meta)+'\n```\n')
        (project/'playbooks').mkdir()
        (project/'playbooks/base.yml').write_text('- name: Base\n  hosts: all_linux_systems:!opnsense_firewall\n')
        (project/'playbooks/docker.yml').write_text('- name: Docker\n  hosts: docker_deployment_hosts\n')
        for name,file in [('base','base.yml'),('docker-deploy','docker.yml')]:
            spec=project/f'bin/control.d/{name}.json';data=json.loads(spec.read_text());data['commands'][0]['requires']=['playbooks/'+file];spec.write_text(json.dumps(data))
        docker_meta={**base_meta,'ref':'docker-deploy','fields':base_meta['fields'][:1],'constraints':[]}
        (docs/'docker-deploy.md').write_text('# Docker\n```json\n'+json.dumps(docker_meta)+'\n```\n')
        state=base/'state'; state.mkdir()
        inventory={'_meta':{'hostvars':{'demo-host':{'ansible_host':'example.invalid','ansible_user':'root','ansible_port':22,'ansible_password':'NEVER_PERSIST_THIS_VALUE'}}},'demo-group':{'hosts':['demo-host']},'docker_deployment_hosts':{'children':['demo-group']}}
        inventory['all_linux_systems']={'hosts':['demo-host']}
        inventory['all_windows_systems']={'hosts':[]}
        inventory.update({f'group-{i:03}':{'hosts':[]} for i in range(120)})
        (project/'.venv/bin').mkdir(parents=True)
        inv=project/'.venv/bin/ansible-inventory'
        fixture=project/'inventory-fixture.json';fixture.write_text(json.dumps(inventory))
        inv.write_text('#!'+sys.executable+'\nimport json,time\nfrom pathlib import Path\np=Path('+repr(str(fixture))+')\nd=p.with_name("inventory-delay")\nif d.exists():time.sleep(float(d.read_text()))\nprint(p.read_text())\n');inv.chmod(0o700)
        ssh=project/'bin/fake-ssh'
        ssh.write_text('#!'+sys.executable+'\n'+'''import json,shlex,sys,time
from pathlib import Path
words=shlex.split(sys.argv[-1])
if words[:2]==['docker','ps']:
 host=sys.argv[-2]
 events=Path(__file__).with_name('ssh-events.jsonl')
 def log(kind):
  with events.open('a') as f:f.write(json.dumps({'event':kind,'host':host,'time':time.monotonic()})+'\\n')
 log('start')
 try:
  if 'slow.invalid' in host:time.sleep(3)
  if 'bad.invalid' in host and not Path(__file__).with_name('bad-ready').exists():sys.exit(5)
  print(json.dumps({'ID':'abc123','Names':'demo-container','Status':'Up 1 minute','Image':'example:latest'}))
 finally:log('end')
elif words[:2]==['docker','stats']:
 print(json.dumps({'Name':'demo-container','CPUPerc':'0.1%','MemUsage':'10MiB / 1GiB'}))
elif words[:2]==['docker','inspect']:
 print(json.dumps({'name':'demo-container','state':'running','ports':{},'mounts':[]}))
else: print('MOCK_REMOTE_ACTION')
''');ssh.chmod(0o700)
        cfg=base/'config.json'; tmux_socket=str(base/'tmux.sock')
        cfg.write_text(json.dumps({'project':str(project),'state':str(state),'tmux_socket':tmux_socket,'refresh_seconds':5,'max_running':6,'tmux':args.tmux,'ssh':str(ssh),'docker_parallel':2}))
        def cli(*words):
            result=subprocess.run([str(binary),'--config',str(cfg),*words],capture_output=True,text=True)
            if result.returncode:
                raise AssertionError(result.stderr)
            return result
        def api(method,**kw):
            request={'method':method,**kw}
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(10);client.connect(str(state/'api.sock'));client.sendall((json.dumps(request)+'\n').encode());f=client.makefile('rb');v=json.loads(f.readline())
            if not v['ok']:
                raise RuntimeError(v['error'])
            return v['data']
        def tmux(*words):
            return subprocess.run([args.tmux,'-S',tmux_socket,*words],check=True,capture_output=True,text=True).stdout
        daemon=None
        def start():
            process=subprocess.Popen([str(binary),'--config',str(cfg),'serve'],stdout=subprocess.DEVNULL,stderr=(base/'service.log').open('a'),env={**os.environ,'PATH':os.environ['PATH']})
            try:
                wait_for(lambda: (state/'api.sock').exists() and process.poll() is None)
                wait_for(lambda: api('status')['version']==expected_version)
                return process
            except BaseException:
                process.terminate();process.wait(timeout=5)
                raise
        try:
            daemon=start();cli('session');tmux('set-window-option','-g','remain-on-exit','on')
            assert len(api('catalog.list'))==5
            duplicate=subprocess.run([str(binary),'--config',str(cfg),'serve'],capture_output=True,timeout=5)
            assert duplicate.returncode!=0
            print('PASS: Dienst, private API, SQLite, Doppelstartschutz')
            command=' '.join(__import__('shlex').quote(s) for s in [str(binary),'--config',str(cfg),'ui'])
            ui=tmux('new-window','-d','-P','-F','#{pane_id}','-n','Verwaltung',command).strip()
            wait_for(lambda:'ANSIBLE TUI' in tmux('capture-pane','-p','-t',ui))
            watchers=[]
            try:
                for _ in range(2):
                    pid,fd=pty.fork()
                    if pid==0:
                        os.environ['TERM']='xterm-256color'
                        os.execv(args.tmux,[args.tmux,'-S',tmux_socket,'attach-session','-r','-t','ansible-tui'])
                    watchers.append((pid,fd))
                wait_for(lambda:tmux('list-clients','-F','#{client_readonly}').splitlines().count('1')==2)
                assert tmux('display-message','-p','-t',ui,'#{pane_dead}').strip()=='0'
                print('PASS: zwei gleichzeitige read-only Teilnehmer derselben tmux-Sitzung')
            finally:
                for pid,fd in watchers:
                    os.kill(pid,signal.SIGTERM)
                    os.close(fd)
                    os.waitpid(pid,0)
            def click_text(text):
                lines=tmux('capture-pane','-p','-t',ui).splitlines()
                matches=[(line.find(text),y) for y,line in enumerate(lines) if text in line and '│' in line[:line.find(text)] and '│' in line[line.find(text)+len(text):]]
                if not matches and 'Alle Aktionen' not in '\n'.join(lines):
                    tmux('send-keys','-t',ui,'C-k')
                    wait_for(lambda:'Alle Aktionen' in tmux('capture-pane','-p','-t',ui))
                    return click_text(text)
                assert matches,(text,lines)
                x,y=matches[-1]
                sequence=f'\x1b[<0;{x+1};{y+1}M\x1b[<0;{x+1};{y+1}m'
                tmux('send-keys','-t',ui,'-l',sequence)
            def click_page(number,label):
                import re
                lines=tmux('capture-pane','-p','-t',ui).splitlines()
                match=next((m,y) for y,line in enumerate(lines) if (m:=re.search(r'\b'+str({1:1,2:2,3:3,4:4,6:5,7:6,8:7}[number])+r'\s+'+label,line)))
                m,y=match;x=m.start()
                tmux('send-keys','-t',ui,'-l',f'\x1b[<0;{x+1};{y+1}M\x1b[<0;{x+1};{y+1}m')
                wait_for(lambda:label in tmux('capture-pane','-p','-t',ui).splitlines()[3][20:])
            tmux('send-keys','-t',ui,'C-g')
            wait_for(lambda:'Tastenhilfe' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'Escape')
            tmux('send-keys','-t',ui,'-l','2')
            wait_for(lambda:'Ansible' in tmux('capture-pane','-p','-t',ui).splitlines()[3][20:])
            tmux('send-keys','-t',ui,'C-k')
            wait_for(lambda:'Alle Aktionen' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'Tab','Right','Enter')
            wait_for(lambda:'Alle Aktionen' not in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'-l','d')
            wait_for(lambda:'Springe zu: d' in tmux('capture-pane','-p','-t',ui))
            assert not api('jobs.list')
            assert 'Konfigurator' not in tmux('capture-pane','-p','-t',ui)
            tmux('send-keys','-t',ui,'Home','Enter')
            wait_for(lambda:'Konfigurator' in tmux('capture-pane','-p','-t',ui))
            wait_for(lambda:'all_linux_systems' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'C-u')
            click_text('Ziele (--limit)')
            wait_for(lambda:'group-119' in api('completion.query',source='inventory.targets',prefix=''))
            wait_for(lambda:len(api('hosts.list'))==1)
            tmux('send-keys','-t',ui,'-l','group-119')
            wait_for(lambda:'group-119' in tmux('capture-pane','-p','-t',ui))
            click_text('group-119')
            click_text('Uebernehmen')
            wait_for(lambda:'Konfigurator' in tmux('capture-pane','-p','-t',ui))
            click_text('Vorschau Ctrl+P')
            wait_for(lambda:'Auftrag pruefen' in tmux('capture-pane','-p','-t',ui))
            assert not api('jobs.list')
            click_text('Starten')
            wait_for(lambda:(project/'base-argv.json').exists())
            assert json.loads((project/'base-argv.json').read_text())==['--limit','group-119']
            wait_for(lambda:any(j['code']==0 for j in api('jobs.list')))
            base_job=next(j for j in api('jobs.list') if j['label']=='Ansible base')
            ui_index=tmux('display-message','-p','-t',ui,'#{window_index}').strip()
            hint=wait_for(lambda: (pane if 'In der Fensterauswahl Verwaltung waehlen.' in (pane:=tmux('capture-pane','-p','-J','-t',base_job['pane'])) else None))
            if len(ui_index)==1:assert 'Direkt: Ctrl+B loslassen, dann '+ui_index+'.' in hint
            assert 'Ctrl+B loslassen, dann w' in hint
            # Reproduce the reported dead-pane situation with a real attached client.
            if len(ui_index)==1:
                tmux('select-window','-t',base_job['pane'])
                job_index=tmux('display-message','-p','-t',base_job['pane'],'#{window_index}').strip()
                pid,fd=pty.fork()
                if pid==0:
                    os.environ['TERM']='xterm-256color'
                    os.execv(args.tmux,[args.tmux,'-S',tmux_socket,'attach-session','-t','ansible-tui'])
                try:
                    wait_for(lambda:bool(tmux('list-clients')))
                    os.write(fd,ui_index.encode());time.sleep(.15)
                    assert tmux('display-message','-p','-t','ansible-tui','#{window_index}').strip()==job_index
                    os.write(fd,b'\x02'+ui_index.encode())
                    wait_for(lambda:tmux('display-message','-p','-t','ansible-tui','#{window_index}').strip()==ui_index)
                finally:
                    os.kill(pid,signal.SIGTERM);os.close(fd);os.waitpid(pid,0)
            print('PASS: Buchstabensprung ohne Jobstart; Auftragsende zeigt passende Ctrl+B-Navigation')

            # UI attaches to the neutral Base job; target pane remains addressable.
            tmux('send-keys','-t',ui,'Escape')
            print('PASS: Base -> --limit -> automatisches Inventory -> Gruppe jenseits Eintrag 50 -> Vorschau -> exakte argv')
            baseline_jobs=len(api('jobs.list'))
            click_page(4,'Docker')

            wait_for(lambda:len(api('hosts.list'))==1)
            wait_for(lambda:'Host waehlen' in tmux('capture-pane','-p','-t',ui))
            click_text('Host waehlen')
            wait_for(lambda:'demo-host' in tmux('capture-pane','-p','-t',ui))
            click_text('demo-host')
            wait_for(lambda:'demo-container' in tmux('capture-pane','-p','-t',ui))
            click_text('Logs')
            wait_for(lambda:'Auftrag pruefen' in tmux('capture-pane','-p','-t',ui))
            assert len(api('jobs.list'))==baseline_jobs
            click_text('Zurueck')
            wait_for(lambda:'Auftrag pruefen' not in tmux('capture-pane','-p','-t',ui))
            assert len(api('jobs.list'))==baseline_jobs
            for number,label in [(1,'Uebersicht'),(2,'Ansible'),(3,'Hosts'),(4,'Docker'),(6,'Sitzungen'),(7,'Verlauf'),(8,'Hilfe')]:
                click_page(number,label)
            print('PASS: echte SGR-Maus fuer alle Bereiche, Inventory-Laden, Docker-Hostauswahl, Logs-Vorschau und Abbrechen')
            tmux('send-keys','-t',ui,'2')
            wait_for(lambda:'demo' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'Down','Enter')
            wait_for(lambda:'Konfigurator' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'Enter')
            wait_for(lambda:'eins' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'Down','Enter','Tab','C-t')
            wait_for(lambda:'Vorlage waehlen' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'Down','Enter','Home')
            tmux('send-keys','-t',ui,'-l','1')
            tmux('send-keys','-t',ui,'C-p')
            wait_for(lambda:'Auftrag pruefen' in tmux('capture-pane','-p','-t',ui))
            assert 'retries: 15' in tmux('capture-pane','-p','-t',ui)
            tmux('send-keys','-t',ui,'Escape','C-q')
            wait_for(lambda:tmux('display-message','-p','-t',ui,'#{pane_dead}').strip()=='1')
            assert len(api('jobs.list'))==baseline_jobs
            print('PASS: echte Hotkeys 2/Ctrl+G/Ctrl+K, Tab/Pfeile/Enter fuer Dialogbuttons, Formularcursor/Home, typgetreue Vorlage bearbeitet, Vorschau/Abbruch ohne Jobstart')
            expanded=json.loads(json.dumps(inventory))
            expanded['_meta']['hostvars'].update({'slow-host':{'ansible_host':'slow.invalid'},'bad-host':{'ansible_host':'bad.invalid'},'excluded':{'ansible_host':'excluded.invalid','proxmox_tags':'docker;deployment-exclude'}})
            expanded['docker_deployment_hosts']={'hosts':['demo-host','slow-host','bad-host']}
            fixture.write_text(json.dumps(expanded));fixture.with_name('inventory-delay').write_text('0.5')
            before=time.monotonic();api('docker.discover',force=True)
            assert time.monotonic()-before<0.5,'Discovery blockiert API'
            assert api('status')['version']==expected_version
            wait_for(lambda:len(api('docker.hosts')['hosts'])==3)
            def states():return {h['host']:h['state'] for h in api('docker.hosts')['hosts']}
            wait_for(lambda:states().get('demo-host')=='ready')
            assert states()['slow-host']!='ready','Langsamer Host soll gesondert laden'
            wait_for(lambda:states().get('slow-host')=='ready')
            wait_for(lambda:states().get('bad-host')=='error')
            summaries=api('docker.hosts')['hosts'];assert all('rows' not in h for h in summaries)
            assert {h['host'] for h in summaries}=={'demo-host','slow-host','bad-host'}
            events=project/'bin/ssh-events.jsonl'
            previous=events.read_text()
            for _ in range(40):
                api('docker.hosts');assert api('docker.snapshot',host='demo-host')['rows'][0]['ID']=='abc123'
            assert events.read_text()==previous,'Cacheleser starten eigene SSH-Abfragen'
            active=maximum=0
            for event in map(json.loads,previous.splitlines()):
                active+=1 if event['event']=='start' else -1;maximum=max(maximum,active)
                assert 'excluded.invalid' not in event['host']
            assert maximum<=2,(maximum,previous)
            assert api('docker.snapshot',host='bad-host')['error']
            # Real UI tabs share cached results with the API and never start jobs.
            ui=tmux('new-window','-d','-P','-F','#{pane_id}','-n','Docker-Cache',command).strip()
            wait_for(lambda:'ANSIBLE TUI' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'4')
            wait_for(lambda:'bad-host' in tmux('capture-pane','-p','-t',ui))
            click_text('Alle');wait_for(lambda:'Docker-Deploymenthost' in tmux('capture-pane','-p','-t',ui))
            click_text('demo-host');wait_for(lambda:'demo-container' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'BTab','Right','Enter')
            wait_for(lambda:'slow-host [' in tmux('capture-pane','-p','-t',ui))
            # Automatic retry recovers an unavailable Docker host without an inventory reload.
            (project/'bin/bad-ready').touch()
            wait_for(lambda:states().get('bad-host')=='ready',timeout=20)
            tmux('send-keys','-t',ui,'C-q')
            wait_for(lambda:tmux('display-message','-p','-t',ui,'#{pane_dead}').strip()=='1')
            fixture.write_text(json.dumps(inventory));fixture.with_name('inventory-delay').unlink()
            api('inventory.refresh');assert {h['host'] for h in api('docker.hosts')['hosts']}=={'demo-host'}
            print('PASS: asynchrone Deployment-Erkennung, Hostreiter/Maus/Tastatur, Poollimit 2, langsame/fehlerhafte Hosts, Cache ohne SSH-Dopplung, automatischer Retry und entfernte Hosts')
            assert api('control.defaults',command='base')['values']=={'targets':['all_linux_systems']}
            assert api('control.defaults',command='docker-deploy')['values']=={'targets':['docker_deployment_hosts']}
            fixture_hash=__import__('hashlib').sha256(fixture.read_bytes()).hexdigest()
            prefs=api('settings.get')['values'];prefs['docker_poll_enabled']=False;prefs['docker_refresh_seconds']=5
            api('settings.update',values=prefs)
            assert api('settings.get')['values']['docker_poll_enabled'] is False
            assert (state/'preferences.json').stat().st_mode&0o777==0o600
            for values in [{**prefs,'docker_parallel':0},{**prefs,'unknown':'disallowed'}]:
                try:api('settings.update',values=values);raise AssertionError('Ungueltige Settings akzeptiert')
                except RuntimeError:pass
            api('inventory.clear',confirm='clear-cache')
            assert not api('hosts.list') and not api('docker.hosts')['hosts']
            assert __import__('hashlib').sha256(fixture.read_bytes()).hexdigest()==fixture_hash
            # An already running inventory process must not repopulate a cleared cache.
            fixture.with_name('inventory-delay').write_text('0.5');api('docker.discover',force=True)
            time.sleep(.15);api('inventory.clear',confirm='clear-cache');time.sleep(.65)
            assert not api('hosts.list')
            fixture.with_name('inventory-delay').unlink()
            # Missing standard group leaves selection open; never falls back to all.
            missing=json.loads(json.dumps(inventory));missing.pop('all_linux_systems');fixture.write_text(json.dumps(missing))
            result=api('control.defaults',command='base');assert result['values']=={} and 'fehlt' in result['hint']
            fixture.write_text(json.dumps(inventory));api('inventory.clear',confirm='clear-cache')
            # Cache operations do not remove jobs or change their identity.
            assert len(api('jobs.list'))==baseline_jobs
            ui=tmux('new-window','-d','-P','-F','#{pane_id}','-n','Settings',command).strip()
            wait_for(lambda:'ANSIBLE TUI' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'-l','9')
            wait_for(lambda:'99_Einstellungen' in tmux('capture-pane','-p','-t',ui).splitlines()[3])
            tmux('send-keys','-t',ui,'Enter')
            wait_for(lambda:'Konfigurator' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'C-p')
            wait_for(lambda:'Auftrag pruefen' in tmux('capture-pane','-p','-t',ui))
            tmux('send-keys','-t',ui,'Escape','Escape','C-q')
            wait_for(lambda:tmux('display-message','-p','-t',ui,'#{pane_dead}').strip()=='1')
            prefs['target_defaults']=False;api('settings.update',values=prefs)
            assert api('control.defaults',command='base')['values']=={}
            print('PASS: Linux-/Docker-Vorgaben ohne all-Fallback, Einstellungen/Validierung, Cacheleeren ohne Dateiaenderung/Jobverlust, laufendes Inventory verworfen, 9 Einstellungen per Tastatur')
            injection='x; touch '+str(base/'MUST_NOT_EXIST')
            job=api('control.start',command='demo',args=[injection])
            pane=job['pane']
            wait_for(lambda:'Auswahl:' in tmux('capture-pane','-p','-t',pane))
            tmux('send-keys','-t',pane,'probe','Enter')
            wait_for(lambda:'Passwort:' in tmux('capture-pane','-p','-t',pane))
            tmux('send-keys','-t',pane,'dummy-hidden','Enter')
            wait_for(lambda:any(j['id']==job['id'] and j['code']==7 for j in api('jobs.list')))
            capture=tmux('capture-pane','-p','-S','-','-t',pane)
            assert 'TTY_OK' in capture and 'DONE' in capture and 'dummy-hidden' not in capture, repr(capture)
            assert json.loads((project/'argv.json').read_text())==[injection]
            assert not (base/'MUST_NOT_EXIST').exists()
            print('PASS: echte PTY-Unterprozesse, /dev/tty, verdeckte Eingabe, Exitcode, sichere Argumente')
            api('inventory.refresh')
            assert 'NEVER_PERSIST_THIS_VALUE' not in json.dumps(api('hosts.list'))
            assert 'demo-host' in api('completion.query',source='inventory.targets',prefix='demo-')
            assert api('docker.list',host='demo-host')['rows'][0]['ID']=='abc123'
            assert api('docker.stats',host='demo-host')['rows'][0]['CPUPerc']=='0.1%'
            assert api('docker.inspect',host='demo-host',container='abc123')['state']=='running'
            for request in [dict(host='demo-host',container='abc123',action='stop'),dict(host='demo-host',container='x; touch /tmp/no',action='logs')]:
                try:
                    api('docker.start',**request)
                    raise AssertionError('Docker-Aktion ohne gueltige Auswahl/Bestaetigung akzeptiert')
                except RuntimeError:
                    pass
            assert b'NEVER_PERSIST_THIS_VALUE' not in (state/'catalog.sqlite3').read_bytes()
            print('PASS: Inventory-/SSH-/Docker-Mocks, Completion, Status/Stats/Inspect und Secret-Ausschluss')
            slow=api('control.start',command='slow',args=[])
            try:
                api('control.start',command='other',args=[])
                raise AssertionError('Ressourcensperre fehlt')
            except RuntimeError as e:
                assert 'belegt' in str(e)
            api('job.capture',id=slow['id'],confirm=slow['id'])
            wait_for(lambda:(state/'jobs'/f"{slow['id']}.log").exists())
            # Simulate a crashing UI, independently of the job window.
            tmux('new-window','-d','-n','Test-UI','sleep 60')
            tmux('kill-window','-t','ansible-tui:Test-UI')
            assert tmux('display-message','-p','-t',slow['pane'],'#{pane_dead}').strip()=='0'
            daemon.kill();daemon.wait(timeout=5);daemon=start()
            assert api('settings.get')['values']['target_defaults'] is False
            assert any(j['id']==slow['id'] for j in api('jobs.list'))
            assert tmux('display-message','-p','-t',slow['pane'],'#{pane_dead}').strip()=='0'
            assert len([j for j in api('jobs.list') if j['id']==slow['id']])==1
            print('PASS: Ressourcensperre, UI-Ende, Dienstabsturz und Wiederverbinden ohne Doppelstart')
            wait_for(lambda:'heartbeat' in api('job.log.read',id=slow['id'])['output'])
            api('job.stop',id=slow['id'],confirm=slow['id'])
            wait_for(lambda:any(j['id']==slow['id'] and j['state'] in ['failed','finished'] for j in api('jobs.list')))
            assert api('events.list')
            print('PASS: begrenztes Ausgabeprotokoll, explizites Ctrl+C, Ereignisverlauf')
            ui=tmux('new-window','-d','-P','-F','#{pane_id}','-n','Quittierung',command).strip()
            wait_for(lambda:'ANSIBLE TUI' in tmux('capture-pane','-p','-t',ui))
            click_page(6,'Sitzungen')
            wait_for(lambda:'Ansible demo' in tmux('capture-pane','-p','-t',ui))
            click_text('Ansible demo')
            click_text('Fehler quittieren')
            wait_for(lambda:'Auftrag pruefen' in tmux('capture-pane','-p','-t',ui))
            click_text('Starten')
            wait_for(lambda:any(j['label']=='Ansible demo' and j.get('acknowledged_at') for j in api('jobs.list')))
            try:wait_for(lambda:'Quittiert' in tmux('capture-pane','-p','-t',ui))
            except AssertionError:
                print('QUITTIERUNG_SCREEN',tmux('capture-pane','-p','-t',ui),flush=True);raise
            click_text('Ausgabelog leeren');click_text('Starten')
            wait_for(lambda:'Ausgabelog samt Rotationen geleert' in tmux('capture-pane','-p','-t',ui))
            click_text('Terminal schliessen');click_text('Starten')
            wait_for(lambda:'Beendetes Terminal geschlossen' in tmux('capture-pane','-p','-t',ui))
            click_page(7,'Verlauf');click_text('Verlauf leeren');click_text('Starten')
            wait_for(lambda:[e['action'] for e in api('events.list')]==['events.cleared'])
            print('PASS: echte Maus fuer Quittieren/gruene Anzeige, Ausgabelog leeren, beendetes Terminal schliessen und Verlaufleeren mit Vorschau')
            # An invalid description is reported and never executed.
            (docs/'bad.md').write_text('```json\n{"schema_version":99,"kind":"command","ref":"demo"}\n```')
            assert api('catalog.refresh')['errors']
            (docs/'demo.md').unlink()
            api('catalog.refresh')
            try:
                api('control.start',command='demo',values={})
                raise AssertionError('Ungueltige Metadaten akzeptiert')
            except RuntimeError:
                pass
            print('PASS: ungueltige Metadaten sichtbar und vor Auftragsstart abgewiesen')
            rss=subprocess.run(['ps','-o','rss=','-p',str(daemon.pid)],capture_output=True,text=True).stdout.strip()
            print('INFO: Koordinator-RSS im neutralen Test, KiB:',rss)
        finally:
            if daemon and daemon.poll() is None:
                daemon.terminate();daemon.wait(timeout=5)
            subprocess.run([args.tmux,'-S',tmux_socket,'kill-server'],capture_output=True)


if __name__=='__main__':
    main()
