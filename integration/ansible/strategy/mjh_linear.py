"""Job-local adapter for tested Ansible 2.19/2.20 linear Docker runs.

No project files or terminal answers are changed. Only the existing summary's
pure acknowledgement becomes a one-second pause. Actual tty input is signalled
without persisting prompts, answers, variables or secrets.
"""
import json
import os
import tempfile
import uuid
from ansible.plugins.strategy.linear import StrategyModule as Linear
from ansible.utils.display import Display

SUMMARY_TAG = 'controller_docker_summary_ack'
LEGACY_TASK = 'TASK 9.3: Zusammenfassung im Terminal anzeigen und auf Bestätigung warten'
LEGACY_PROMPT = ('Deployment Zusammenfassung (Logfile-Inhalt):', 'Drücken Sie Enter, um fortzufahren.')
SUMMARY_PROMPT = ("Deployment summary (log contents):\n"
                  "{{ lookup('file', log_file) }}\n"
                  "Press Enter to continue.")


def is_summary_ack(task):
    args = task.args or {}
    if (task.action not in ('pause', 'ansible.builtin.pause', 'ansible.legacy.pause')
            or set(args) - {'prompt', 'echo'} or args.get('echo', True) is not True
            or task.register or not isinstance(args.get('prompt'), str)):
        return False
    if SUMMARY_TAG in (task.tags or ()):
        return args['prompt'].strip() == SUMMARY_PROMPT
    lines = [line.strip() for line in args['prompt'].splitlines()
             if line.strip() and set(line.strip()) != {'='}]
    if len(lines) != 3 or lines[1] not in ("{{ lookup('file', log_file) }}", '{{ lookup("file", log_file) }}'):
        return False
    prompt = lines[0], lines[2]
    return task.get_name().strip() == LEGACY_TASK and prompt == LEGACY_PROMPT


def signal(state, token=''):
    path = os.environ.get('MJH_INTERACTION_FILE')
    if not path:
        return
    fd, temp = tempfile.mkstemp(prefix='.mjh-input-', dir=os.path.dirname(path))
    with os.fdopen(fd, 'w') as stream:
        json.dump({'state': state, 'token': token}, stream)
    os.replace(temp, path)


class StrategyModule(Linear):
    def run(self, iterator, play_context):
        original = Display.prompt_until
        owner = os.getpid()

        def prompt(display, *args, **kwargs):
            seconds = kwargs.get('seconds', args[2] if len(args) > 2 else None)
            # Worker calls retain Ansible's proxy; only the controller tty read
            # emits a phase. Skipped tasks and timed waits never request focus.
            if os.getpid() != owner or seconds is not None:
                return original(display, *args, **kwargs)
            signal('input', uuid.uuid4().hex)
            try:
                return original(display, *args, **kwargs)
            finally:
                signal('running')

        signal('running')
        Display.prompt_until = prompt
        try:
            return super().run(iterator, play_context)
        finally:
            Display.prompt_until = original
            signal('running')

    def _queue_task(self, host, task, task_vars, play_context):
        if is_summary_ack(task) and os.environ.get('MJH_AUTO_ACK') == '1':
            task = task.copy()
            task.args = {**task.args, 'seconds': 1}
            Display().display('MJH: Zusammenfassung angezeigt; reine Enter-Pause automatisch fortsetzen.')
        return super()._queue_task(host, task, task_vars, play_context)
