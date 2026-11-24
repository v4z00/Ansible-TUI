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

SUMMARY_TASK = 'TASK 9.3: Zusammenfassung im Terminal anzeigen und auf Bestätigung warten'
SUMMARY_END = 'Drücken Sie Enter, um fortzufahren.'


def is_summary_ack(task):
    args = task.args or {}
    prompt = args.get('prompt')
    lines = [line.strip() for line in prompt.splitlines()
             if line.strip() and set(line.strip()) != {'='}] if isinstance(prompt, str) else []
    return (task.action in ('pause', 'ansible.builtin.pause', 'ansible.legacy.pause')
            and task.get_name().strip() == SUMMARY_TASK
            and args.get('seconds') is None and args.get('minutes') is None
            and args.get('echo', True) is True
            and len(lines) == 3
            and lines[0] == 'Deployment Zusammenfassung (Logfile-Inhalt):'
            and lines[1] in ("{{ lookup('file', log_file) }}", '{{ lookup("file", log_file) }}')
            and lines[2] == SUMMARY_END)


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
