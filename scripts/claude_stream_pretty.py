#!/usr/bin/env python3
import json
import os
import sys
from typing import Any, Dict, Optional


def _println(s: str) -> None:
    sys.stdout.write(s + "\n")
    sys.stdout.flush()


def _print(s: str) -> None:
    sys.stdout.write(s)
    sys.stdout.flush()


def _truncate(s: str, n: int) -> str:
    if len(s) <= n:
        return s
    return s[: n - 1] + "…"


def _json_compact(v: Any, max_len: int) -> str:
    try:
        s = json.dumps(v, ensure_ascii=False, separators=(",", ":"))
    except Exception:
        s = repr(v)
    return _truncate(s, max_len)


class Printer:
    def __init__(self) -> None:
        self._buf: str = ""
        self._tool_input_max = int(os.getenv("CLAUDE_PRETTY_TOOL_INPUT_MAX", "800"))

    def flush(self) -> None:
        if self._buf:
            _println(self._buf)
            self._buf = ""

    def on_text_delta(self, t: str) -> None:
        self._buf += t
        while "\n" in self._buf:
            line, rest = self._buf.split("\n", 1)
            _println(line)
            self._buf = rest

    def on_tool_use(self, name: str, tool_input: Any, tool_id: Optional[str]) -> None:
        self.flush()
        suffix = f" id={tool_id}" if tool_id else ""
        _println(f"\n>>> tool_use {name}{suffix}")
        if tool_input is not None:
            _println(_json_compact(tool_input, self._tool_input_max))

    def on_final_message(self, text: str) -> None:
        # This is typically the non-streamed final assistant text.
        self.flush()
        if text.strip():
            _println(text.rstrip("\n"))

    def on_result(self, ok: bool, result_text: str) -> None:
        self.flush()
        status = "success" if ok else "error"
        _println(f"\n=== {status} ===")
        if result_text.strip():
            _println(result_text.rstrip("\n"))


def _get(d: Dict[str, Any], *keys: str) -> Any:
    cur: Any = d
    for k in keys:
        if not isinstance(cur, dict) or k not in cur:
            return None
        cur = cur[k]
    return cur


def main() -> int:
    p = Printer()

    for raw in sys.stdin:
        line = raw.strip()
        if not line:
            continue

        try:
            obj = json.loads(line)
        except Exception:
            # If something non-JSON sneaks in, keep it visible.
            p.flush()
            _println(line)
            continue

        t = obj.get("type")

        if t == "stream_event":
            delta_type = _get(obj, "event", "delta", "type")
            if delta_type == "text_delta":
                text = _get(obj, "event", "delta", "text") or ""
                if text:
                    p.on_text_delta(text)
            # Ignore input_json_delta + other noisy internal events by default.
            continue

        if t == "assistant":
            msg = obj.get("message") or {}
            content = msg.get("content") or []
            for block in content:
                if not isinstance(block, dict):
                    continue
                bt = block.get("type")
                if bt == "tool_use":
                    p.on_tool_use(
                        str(block.get("name") or "unknown"),
                        block.get("input"),
                        block.get("id"),
                    )
                elif bt == "text":
                    text = block.get("text") or ""
                    if text:
                        p.on_final_message(str(text))
            continue

        if t == "result":
            ok = not bool(obj.get("is_error"))
            result_text = obj.get("result") or ""
            p.on_result(ok, str(result_text))
            continue

        # Everything else (rate_limit_event, etc.) is ignored.

    p.flush()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

