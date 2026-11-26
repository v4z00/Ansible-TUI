#!/usr/bin/env python3
"""Real local Ansible in a PTY: auto ack, timed wait, skipped input, real choice."""
import argparse,json,os,pty,select,signal,subprocess,tempfile,time
from pathlib import Path

def check_controller_contract(controller):
 """Inspect the actual controller playbook without evaluating or running its tasks."""
 import importlib.util
 from types import SimpleNamespace
 import yaml

 plugin = Path(__file__).resolve().parents[1] / 'integration/ansible/strategy/mjh_linear.py'
 spec = importlib.util.spec_from_file_location('docker_strategy', plugin)
 strategy = importlib.util.module_from_spec(spec)
 spec.loader.exec_module(strategy)
 playbook = controller / 'playbooks/docker/deploy_docker_stacks.yml'
 tasks = [task for play in yaml.safe_load(playbook.read_text()) for task in play.get('tasks', [])]
 pauses = [task for task in tasks if 'ansible.builtin.pause' in task]
 summary = [task for task in pauses if strategy.SUMMARY_TAG in task.get('tags', [])]
 assert len(summary) == 1, 'Expected one tagged Docker deployment summary'
 orphan = [task for task in pauses if task.get('register') == 'global_prune_confirmation']
 assert len(orphan) == 1, 'Expected one orphan stack confirmation'

 def runtime(task):
  return SimpleNamespace(action='ansible.builtin.pause', args=task['ansible.builtin.pause'],
                         register=task.get('register'), tags=task.get('tags', []),
                         get_name=lambda:task.get('name',''))

 assert [task for task in pauses if strategy.is_summary_ack(runtime(task))] == summary
 ack = runtime(summary[0])
 ack.tags = []
 assert not strategy.is_summary_ack(ack)
 ack = runtime(summary[0])
 ack.register = 'answer'
 assert not strategy.is_summary_ack(ack)
 # Even an incorrectly tagged copy of the orphan prompt must never be acknowledged.
 danger = runtime(orphan[0])
 danger.tags = [strategy.SUMMARY_TAG]
 danger.register = None
 assert not strategy.is_summary_ack(danger)


def main():
 p=argparse.ArgumentParser();p.add_argument('--ansible',type=Path,required=True)
 p.add_argument('--controller',type=Path,help='Also validate the checked-out controller Docker playbook')
 args=p.parse_args()
 root=Path(__file__).resolve().parents[1]
 with tempfile.TemporaryDirectory(prefix='mjh-real-ansible-') as tmp:
  base=Path(tmp);(base/'summary.txt').write_text('NEUTRALE TESTZUSAMMENFASSUNG')
  if args.controller:
   from unittest.mock import patch
   with patch.dict(os.environ, {'ANSIBLE_LOCAL_TEMP':str(base/'contract-tmp'),'ANSIBLE_CONFIG':str(base/'ansible.cfg')}):
    check_controller_contract(args.controller)
  marker=base/'interaction.json'
  play=base/'test.yml';play.write_text('''- name: Lokaler Adaptertest
  hosts: localhost
  connection: local
  gather_facts: false
  vars:
    log_file: "'''+str(base/'summary.txt')+'''"
  tasks:
    - name: "TASK 9.3: Zusammenfassung im Terminal anzeigen und auf Bestätigung warten"
      ansible.builtin.pause:
        prompt: |
          ======================================================================
          Deployment Zusammenfassung (Logfile-Inhalt):
          {{ lookup('file', log_file) }}
          Drücken Sie Enter, um fortzufahren.
    - name: Display deployment results
      tags: [controller_docker_summary_ack]
      ansible.builtin.pause:
        prompt: |
          Deployment summary (log contents):
          {{ lookup('file', log_file) }}
          Press Enter to continue.
    - name: "Zeitpause unveraendert"
      ansible.builtin.pause:
        seconds: 2
        prompt: "TIMED_WAIT_TEST"
    - name: "Uebersprungene Frage"
      ansible.builtin.pause:
        prompt: "SKIPPED_QUESTION_MUST_NOT_WAIT"
      when: false
    - name: Confirm orphan stack removal
      tags: [controller_docker_summary_ack]
      ansible.builtin.pause:
        prompt: "CHOICE_TEST Type exactly yes to remove orphan stacks."
      register: answer
    - name: Auswahl pruefen
      ansible.builtin.assert:
        that: "answer.user_input == 'no'"
''')
  env={**os.environ,'ANSIBLE_LOCAL_TEMP':str(base/'local-tmp'),'ANSIBLE_STRATEGY_PLUGINS':str(root/'integration/ansible/strategy'),'ANSIBLE_STRATEGY':'mjh_linear','MJH_AUTO_ACK':'1','MJH_INTERACTION_FILE':str(marker),'ANSIBLE_NOCOLOR':'1','ANSIBLE_CONFIG':str(base/'ansible.cfg')}
  (base/'ansible.cfg').write_text('[defaults]\nhost_key_checking=true\n')
  pid,fd=pty.fork()
  if pid==0:os.execve(str(args.ansible),[str(args.ansible),'-i','localhost,',str(play)],env)
  output=b'';sent=False;first_choice=None;start=time.monotonic();exitcode=None
  try:
   while time.monotonic()-start<35:
    if select.select([fd],[],[],.05)[0]:
     try:output+=os.read(fd,65536)
     except OSError:break
    value=json.loads(marker.read_text()) if marker.exists() else {}
    if value.get('state')=='input' and not sent:
     assert b'NEUTRALE TESTZUSAMMENFASSUNG' in output,output.decode(errors='replace')
     assert b'TIMED_WAIT_TEST' in output
     if first_choice is None:first_choice=time.monotonic()
     if time.monotonic()-first_choice>.35:
      os.write(fd,b'no\n');sent=True
    done,status=os.waitpid(pid,os.WNOHANG)
    if done:exitcode=os.waitstatus_to_exitcode(status);break
   if exitcode is None:
    done,status=os.waitpid(pid,os.WNOHANG)
    if done:exitcode=os.waitstatus_to_exitcode(status)
   assert sent and exitcode==0,(sent,exitcode,output.decode(errors='replace'))
   assert time.monotonic()-start>=3,'Feste Pause wurde verkuerzt'
   assert json.loads(marker.read_text())['state']=='running'
   version=subprocess.check_output([str(args.ansible),'--version'],env=env,text=True).splitlines()[0]
   print('PASS:',version,'pure Enter-Zusammenfassung automatisch, echte Auswahl unveraendert, Zeitpause und Skip erhalten, Phasenmarkierung ohne Eingabeprotokoll')
  finally:
   try:os.kill(pid,signal.SIGTERM);os.waitpid(pid,0)
   except (ProcessLookupError,ChildProcessError):pass
   os.close(fd)
if __name__=='__main__':main()
