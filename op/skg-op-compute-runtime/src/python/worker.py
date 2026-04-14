"""
Skelegent compute runtime Python worker (Task 4).

Speaks a minimal length-prefixed JSON protocol over stdio:
- Requests: {"op": "init"|"exec"|"reset"|"close", ...}
- Responses: { ok, stdout, stderr, exit_code, final_result, notes, error }

Maintains a persistent namespace between execs. `reset` clears it and reinstalls the prelude.
"""

import io
import json
import sys
import traceback
from contextlib import redirect_stdout, redirect_stderr

# 4-byte big-endian length-prefixed JSON messages
def read_msg(inp):
    len_bytes = inp.read(4)
    if not len_bytes or len(len_bytes) < 4:
        raise EOFError
    length = int.from_bytes(len_bytes, byteorder="big")
    data = inp.read(length)
    if data is None or len(data) < length:
        raise EOFError
    return json.loads(data)

def write_msg(out, obj):
    data = json.dumps(obj, separators=(",", ":")).encode("utf-8")
    out.write(len(data).to_bytes(4, byteorder="big"))
    out.write(data)
    out.flush()

class Worker:
    def __init__(self):
        self.ns = {}
        self.prelude_installed = False
        self.last_prelude = None

    def _install_prelude(self, prelude_src: str):
        self.ns = {}
        self.ns["__SKG_RESULT"] = {"final": None, "notes": []}
        try:
            exec(prelude_src, self.ns, self.ns)
            self.prelude_installed = True
            self.last_prelude = prelude_src
            return True, None
        except Exception as e:
            return False, f"prelude error: {e}"

    def _exec(self, code: str):
        self.ns["__SKG_RESULT"] = {"final": None, "notes": []}
        stdout_buf = io.StringIO()
        stderr_buf = io.StringIO()
        ok = True
        exit_code = 0
        try:
            with redirect_stdout(stdout_buf), redirect_stderr(stderr_buf):
                exec(code, self.ns, self.ns)
        except SystemExit as e:
            ok = False
            exit_code = int(e.code) if isinstance(e.code, int) else 1
            traceback.print_exc(file=stderr_buf)
        except Exception:
            ok = False
            exit_code = 1
            traceback.print_exc(file=stderr_buf)
        result = self.ns.get("__SKG_RESULT", {})
        final_val = result.get("final")
        notes = result.get("notes", [])
        return {
            "ok": ok,
            "stdout": stdout_buf.getvalue(),
            "stderr": stderr_buf.getvalue(),
            "exit_code": exit_code,
            "final_result": final_val,
            "notes": notes,
        }

    def handle(self, req):
        op = req.get("op")
        if op == "init":
            prelude = req.get("prelude", "")
            ok, err = self._install_prelude(prelude)
            if ok:
                write_msg(sys.stdout.buffer, {"ok": True})
            else:
                write_msg(sys.stdout.buffer, {"ok": False, "error": err})
        elif op == "exec":
            if not self.prelude_installed:
                write_msg(sys.stdout.buffer, {"ok": False, "error": "prelude not installed"})
            else:
                code = req.get("code", "")
                resp = self._exec(code)
                resp["ok"] = resp.get("exit_code", 0) == 0
                write_msg(sys.stdout.buffer, resp)
        elif op == "reset":
            if self.last_prelude is not None:
                self._install_prelude(self.last_prelude)
                write_msg(sys.stdout.buffer, {"ok": True})
            else:
                self.ns = {}
                self.prelude_installed = False
                write_msg(sys.stdout.buffer, {"ok": True})
        elif op == "close":
            write_msg(sys.stdout.buffer, {"ok": True})
            sys.exit(0)
        else:
            write_msg(sys.stdout.buffer, {"ok": False, "error": f"unknown op: {op}"})

def main():
    w = Worker()
    while True:
        try:
            req = read_msg(sys.stdin.buffer)
        except EOFError:
            break
        w.handle(req)

if __name__ == "__main__":
    main()
