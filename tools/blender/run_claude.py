"""One model request made by Claude Code, unattended, with its log: what the map editor's
Requests window starts with its Start button (and stops with Cancel), or by hand:

    python tools/blender/run_claude.py ID [--model opus|sonnet|haiku]

Claude Code (`claude`: CLAUDE names the program, else the PATH, else ~/.local/bin) gets the
recipe (requests.md) for that one request and only the tools it needs, set in the request's
claude-settings.json: reading the project, writing in the request's folder, Blender through
run_request.py, model_requests.py and the content check. Anything else is refused, never
asked (nobody is there to answer). Everything it does goes into the request's log.txt, a line
each, timed by the computer's clock: Claude reached and the model it runs, what was sent,
each message, tool call and answer, the end (the raw stream in run.jsonl). Meanwhile the
request is in progress; it ends done once Claude has added the model; failed when Claude ends
without it, stops with an error, or says nothing for 30 minutes (then it's stopped); waiting
again when a file named "stop" appears in its folder (the editor's Cancel).
"""
import json
import os
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import model_requests as mr  # noqa: E402

# seconds without a word from Claude: the run is stopped, the request failed
QUIET = 30 * 60
# lines of a tool's answer kept in the log (run.jsonl has everything)
SHOWN = 30

PROMPT = """You are modelling one request of the Battle Frontier map editor, unattended: nobody can
answer a question, so decide for yourself, and say what you guessed in the reply.

The request: "{name}", a {kind} {height} cells tall, {style}, in private/requests/{id}/ (its
request.json and its pictures). Notes from the user: {notes}

Follow tools/blender/requests.md for this one request, from its step 3 (it's started already):
- read private/requests/{id}/request.json and look at every picture in that folder;
- write private/requests/{id}/model.py with tools/blender/request_kit.py;
- render it: python tools/blender/run_request.py {id}
- compare private/requests/{id}/compare.png and the previews with the pictures, change model.py
  and render again until it matches (usually three to six tries);
- a creature: rig it and give it its idle (requests.md, step 7);
- add it to the project: python tools/blender/run_request.py {id} --finish
- check: cargo run -q -p engine-content --bin content -- check

Run commands with the Bash tool, exactly as written above. Write only inside
private/requests/{id}/. Don't use git, don't change anything else in the project (finishing
writes the prop itself), don't touch other requests. End with one line saying what you made."""


def find_claude():
    env = os.environ.get("CLAUDE")
    if env and Path(env).is_file():
        return env
    for name in ("claude", "claude.exe", "claude.cmd"):
        found = shutil.which(name)
        if found:
            return found
    home = Path.home()
    for p in (home / ".local" / "bin" / "claude.exe", home / ".local" / "bin" / "claude", home / ".claude" / "local" / "claude",
              Path(os.environ.get("APPDATA", str(home))) / "npm" / "claude.cmd"):
        if p.is_file():
            return str(p)
    return None


def program(claude):
    """How to start it: a .cmd through cmd, a .py (the tests' stand-in) through Python."""
    low = claude.lower()
    if low.endswith(".py"):
        return [sys.executable, claude]
    if low.endswith((".cmd", ".bat")):
        return ["cmd", "/c", claude]
    return [claude]


def allowed(rid):
    """What the run may do without asking; everything else is refused."""
    here = f"private/requests/{rid}"
    return [
        "Read", "Glob", "Grep",
        f"Write({here}/**)", f"Edit({here}/**)",
        f"Bash(python tools/blender/run_request.py {rid})", f"Bash(python tools/blender/run_request.py {rid} *)",
        "Bash(python tools/blender/model_requests.py *)",
        "Bash(cargo run -q -p engine-content --bin content -- check)",
        "Bash(ls)", "Bash(ls *)",
    ]


def clip(text, n=SHOWN):
    lines = text.splitlines()
    return text if len(lines) <= n else "\n".join(lines[:n]) + f"\n... ({len(lines) - n} more lines)"


def flat(content):
    """A tool's answer as text (a picture as a word)."""
    if isinstance(content, str):
        return content
    out = []
    for c in content or []:
        if isinstance(c, dict) and c.get("type") == "text":
            out.append(c.get("text", ""))
        elif isinstance(c, dict) and c.get("type") == "image":
            out.append("(a picture)")
        else:
            out.append(json.dumps(c, ensure_ascii=False)[:200])
    return "\n".join(out)


def what(name, args):
    """A tool call in a line: its command, file or pattern."""
    key = {"Bash": "command", "PowerShell": "command", "Read": "file_path", "Write": "file_path", "Edit": "file_path", "Glob": "pattern", "Grep": "pattern"}.get(name)
    if key and key in args:
        return str(args[key]) + (f"   ({args['description']})" if args.get("description") else "")
    return json.dumps(args, ensure_ascii=False)[:300]


def describe(event, run):
    """What one event of Claude's stream says, as lines of the log; what the run learns."""
    kind = event.get("type")
    if kind == "system" and event.get("subtype") == "init":
        run["model"] = event.get("model")
        return [f"Claude reached: model {event.get('model')}, Claude Code {event.get('claude_code_version', '?')}, "
                f"permission mode {event.get('permissionMode')}, session {event.get('session_id')}"]
    if kind == "system":
        return [f"(Claude Code: {event.get('subtype')})"]
    if kind == "assistant":
        if event.get("error"):
            run["error"] = event["error"]
        out = []
        for c in event.get("message", {}).get("content", []):
            if c.get("type") == "text" and c.get("text", "").strip():
                out.append("Claude: " + c["text"].strip())
            elif c.get("type") == "thinking" and c.get("thinking", "").strip():
                out.append("Claude thinks: " + clip(c["thinking"].strip(), 12))
            elif c.get("type") == "tool_use":
                out.append(f"→ {c.get('name')}: {what(c.get('name'), c.get('input', {}))}")
        return out
    if kind == "user":
        content = event.get("message", {}).get("content", [])
        out = []
        for c in content if isinstance(content, list) else []:
            if isinstance(c, dict) and c.get("type") == "tool_result":
                out.append(("× " if c.get("is_error") else "← ") + clip(flat(c.get("content")).strip() or "(nothing)"))
        return out
    if kind == "result":
        run["result"] = event
        secs = (event.get("duration_ms") or 0) / 1000
        cost = event.get("total_cost_usd")
        out = [f"Claude finished ({event.get('subtype')}{', with an error' if event.get('is_error') else ''}): {event.get('num_turns')} turns, "
               f"{int(secs // 60)} min {int(secs % 60)} s" + (f", ${cost:.2f} at API prices" if cost else "")]
        denials = event.get("permission_denials") or []
        if denials:
            out.append("Refused, not allowed in this run: " + "; ".join(f"{d.get('tool_name')} {what(d.get('tool_name'), d.get('tool_input') or {})}" for d in denials))
        if event.get("result"):
            out.append("Its last word: " + clip(str(event["result"]).strip(), 12))
        return out
    return []


def stop_tree(proc):
    """Claude and what it started (Blender, a shell)."""
    if os.name == "nt":
        subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], capture_output=True)
    else:
        try:
            os.killpg(proc.pid, 9)
        except OSError:
            proc.kill()


def run(root, rid, model=None, quiet=QUIET):
    """The run, start to end; the request's status as it ended."""
    record = mr.load(root, rid)
    here = mr.folder(root, rid)
    stop = here / "stop"
    if stop.exists():
        stop.unlink()

    def say(text):
        mr.log(root, rid, text)

    say("———————— a run starts ————————")
    claude = find_claude()
    if not claude:
        return mr.fail(root, rid, "Claude Code not found: install it, or set CLAUDE to the claude program")["status"]
    try:
        version = subprocess.run(program(claude) + ["--version"], capture_output=True, text=True, timeout=60).stdout.strip() or "?"
    except (OSError, subprocess.TimeoutExpired):
        version = "?"
    settings = here / "claude-settings.json"
    settings.write_text(json.dumps({"permissions": {"allow": allowed(rid)}}, indent=2) + "\n", encoding="utf-8")
    args = ["-p", "--output-format", "stream-json", "--verbose", "--permission-mode", "dontAsk", "--strict-mcp-config", "--no-session-persistence",
            "--settings", str(settings.relative_to(root))]
    if model:
        args += ["--model", model]
    prompt = PROMPT.format(id=rid, name=record["name"], kind=record["kind"], height=record["height"], style=record["style"].replace("_", "-"),
                           notes=record["notes"].strip() or "(none)")
    mr.start(root, rid)
    mr.step(root, rid, "Claude is starting", 0.05)
    say(f"Claude Code: {claude} ({version})")
    say(f"Sent: claude {' '.join(args)}   (the prompt on its input; model: {model or 'its default'})")
    say("Allowed without asking: " + ", ".join(allowed(rid)))
    say("Prompt:\n" + prompt)
    extra = {} if os.name == "nt" else {"start_new_session": True}
    proc = subprocess.Popen(program(claude) + args, cwd=str(root), stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, encoding="utf-8", errors="replace", bufsize=1, **extra)
    state = {"last": time.time(), "model": None, "result": None, "error": None}
    lock = threading.Lock()
    raw = open(here / "run.jsonl", "a", encoding="utf-8", newline="\n")

    def out():
        for line in proc.stdout:
            with lock:
                state["last"] = time.time()
                raw.write(line)
                raw.flush()
                try:
                    event = json.loads(line)
                except ValueError:
                    if line.strip():
                        say(line.rstrip())
                    continue
                for text in describe(event, state):
                    say(text)

    def err():
        for line in proc.stderr:
            with lock:
                state["last"] = time.time()
                if line.strip():
                    say("(Claude Code says) " + line.rstrip())

    readers = [threading.Thread(target=out, daemon=True), threading.Thread(target=err, daemon=True)]
    for t in readers:
        t.start()
    try:
        proc.stdin.write(prompt)
        proc.stdin.close()
    except OSError:
        pass
    ended = None
    while proc.poll() is None:
        time.sleep(0.5)
        if stop.exists():
            stop_tree(proc)
            ended = "cancelled"
        elif time.time() - state["last"] > quiet:
            stop_tree(proc)
            ended = "quiet"
        if ended:
            break
    proc.wait()
    for t in readers:
        t.join(timeout=10)
    raw.close()
    if ended == "cancelled":
        stop.unlink(missing_ok=True)
        say("Cancelled from the editor: Claude stopped, the request waits again")
        return mr.update(root, rid, status="waiting", progress=dict(mr.FIELDS["progress"]))["status"]
    if ended == "quiet":
        return mr.fail(root, rid, f"no word from Claude for {max(1, round(quiet / 60))} min, so it was stopped")["status"]
    record = mr.load(root, rid)
    result = state["result"] or {}
    if record["status"] == "done":
        say(f"Done: the prop {record['prop']}")
        return "done"
    last = str(result.get("result") or "").strip()
    if state["error"] == "authentication_failed" or "Not logged in" in last:
        reason = "Claude Code isn't logged in: run claude in a terminal and type /login, once"
    elif proc.returncode != 0 or result.get("is_error"):
        reason = f"Claude stopped with an error ({result.get('subtype') or 'no result'}, exit code {proc.returncode})"
    else:
        reason = "Claude ended without adding the model"
    if last:
        mr.update(root, rid, reply=last)
    return mr.fail(root, rid, reason)["status"]


def main(argv):
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if not argv or argv[0].startswith("-"):
        print(__doc__)
        return 2
    rid, model, quiet = argv[0], None, QUIET
    if "--model" in argv:
        model = argv[argv.index("--model") + 1]
    if "--quiet" in argv:  # seconds, for the tests
        quiet = float(argv[argv.index("--quiet") + 1])
    root = mr.find_root(Path.cwd())
    status = run(root, rid, model, quiet)
    print(f"{rid}: {status}")
    return 0 if status in ("done", "waiting") else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
