#!/usr/bin/env python3
"""A stand-in for `claude` that speaks the stream-json protocol, for tests.

Behaviour is chosen by the text of the user's message:
  echo ...    answer "echo: <text>"
  stream      send three text deltas, then the message
  tool        ask permission to Write a file, then say what happened
  bash <cmd>  ask permission to run <cmd> with Bash
  slow        stream until interrupted (or 20 seconds)
  steer       like slow, but a second message during the turn is added to the answer
  crash       print some output and exit with code 3 in the middle of the turn
  think       send a thinking block, text and a tool call in one message
  fail        end the turn with an error result
  mcp         send a control request the host does not support and wait for the reply
  big         send one assistant message of about 300 KB
  bye         exit cleanly
Everything the process is asked to do is appended to $FAKE_CLAUDE_LOG as JSON lines.
"""
import json
import os
import queue
import sys
import threading
import time
import uuid

argv = sys.argv[1:]
log_path = os.environ.get("FAKE_CLAUDE_LOG")


def log(entry):
    if log_path:
        with open(log_path, "a") as f:
            f.write(json.dumps(entry) + "\n")


def arg_value(flag):
    return argv[argv.index(flag) + 1] if flag in argv else None


log({"argv": argv, "cwd": os.getcwd(), "env": {k: v for k, v in os.environ.items() if k.startswith("CIRCLES_")}})

session_id = arg_value("--session-id") or arg_value("--resume") or str(uuid.uuid4())
if "--fork-session" in argv and not arg_value("--session-id"):
    session_id = str(uuid.uuid4())
model = arg_value("--model") or "fake-model"
mode = arg_value("--permission-mode") or "default"
inbox = queue.Queue()


def emit(obj):
    obj.setdefault("session_id", session_id)
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def reader():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except ValueError:
            continue
        log({"received": message})
        inbox.put(message)
    inbox.put(None)


threading.Thread(target=reader, daemon=True).start()


def init():
    emit({
        "type": "system", "subtype": "init", "cwd": os.getcwd(), "model": model,
        "permissionMode": mode, "claude_code_version": "0.0.0-fake",
        "tools": ["Read", "Write", "Bash"], "slash_commands": ["compact"],
    })


def text_of(message):
    content = message.get("message", {}).get("content")
    if isinstance(content, str):
        return content
    return " ".join(b.get("text", "") for b in content or [] if b.get("type") == "text")


def assistant(blocks):
    emit({"type": "assistant", "message": {"id": "msg_" + uuid.uuid4().hex[:8], "role": "assistant", "content": blocks}, "parent_tool_use_id": None})


def result(is_error=False, text=None, cost=0.01):
    emit({
        "type": "result", "subtype": "error_during_execution" if is_error else "success",
        "is_error": is_error, "result": text, "total_cost_usd": cost, "duration_ms": 5,
        "stop_reason": None if is_error else "end_turn", "terminal_reason": "completed",
    })


def wait_for(predicate, timeout=30):
    """Waits for a message from stdin that satisfies `predicate`, keeping the others for later."""
    deadline = time.time() + timeout
    held = []
    try:
        while time.time() < deadline:
            try:
                message = inbox.get(timeout=0.05)
            except queue.Empty:
                continue
            if message is None:
                return None
            if predicate(message):
                return message
            held.append(message)
        return None
    finally:
        for message in held:
            inbox.put(message)


def is_control_response(message):
    return message.get("type") == "control_response"


def handle_control(message):
    request = message.get("request", {})
    sub = request.get("subtype")
    emit({"type": "control_response", "response": {"subtype": "success", "request_id": message.get("request_id"), "response": {}}, "session_id": None})
    if sub == "set_permission_mode":
        log({"mode_set": request.get("mode")})


def ask_permission(tool, tool_input):
    tool_use_id = "toolu_" + uuid.uuid4().hex[:8]
    assistant([{"type": "tool_use", "id": tool_use_id, "name": tool, "input": tool_input}])
    request_id = "req_" + uuid.uuid4().hex[:8]
    emit({"type": "control_request", "request_id": request_id, "request": {
        "subtype": "can_use_tool", "tool_name": tool, "display_name": tool, "input": tool_input, "tool_use_id": tool_use_id}})
    answer = wait_for(lambda m: is_control_response(m) and m.get("response", {}).get("request_id") == request_id)
    if answer is None:
        sys.exit(0)
    decision = answer["response"]["response"]
    allowed = decision.get("behavior") == "allow"
    emit({"type": "user", "message": {"role": "user", "content": [{
        "tool_use_id": tool_use_id, "type": "tool_result",
        "content": "ok" if allowed else decision.get("message", "denied"), **({} if allowed else {"is_error": True})}]}, "parent_tool_use_id": None})
    assistant([{"type": "text", "text": f"{tool} done" if allowed else f"{tool} denied"}])


def interrupted():
    emit({"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": "[Request interrupted by user]"}]}, "parent_tool_use_id": None})
    result(cost=0)


def turn(message):
    text = text_of(message)
    init()
    word = text.split()[0] if text.split() else ""
    if word == "echo":
        emit({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed", "resetsAt": 1791438600, "rateLimitType": "five_hour", "isUsingOverage": False,
                                                              "unifiedWindows": {"five_hour": {"utilization": 0.4, "resetsAt": 1791438600}, "seven_day": {"utilization": 0.1, "resetsAt": 1792008000}}}})
        assistant([{"type": "text", "text": "echo: " + text[5:]}])
        result()
    elif word == "mcp":
        emit({"type": "control_request", "request_id": "req_mcp", "request": {"subtype": "mcp_message", "server_name": "x"}})
        answer = wait_for(lambda m: is_control_response(m) and m.get("response", {}).get("request_id") == "req_mcp")
        log({"mcp_answer": answer["response"] if answer else None})
        assistant([{"type": "text", "text": "mcp handled"}])
        result()
    elif word == "stream":
        for part in ("alpha ", "beta ", "gamma"):
            emit({"type": "stream_event", "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": part}}, "parent_tool_use_id": None})
            time.sleep(0.02)
        assistant([{"type": "text", "text": "alpha beta gamma"}])
        result()
    elif word == "tool":
        ask_permission("Write", {"file_path": os.path.join(os.getcwd(), "out.txt"), "content": "hello"})
        result()
    elif word == "bash":
        ask_permission("Bash", {"command": text[5:].strip() or "ls"})
        result()
    elif word in ("slow", "steer"):
        steered = []
        deadline = time.time() + 20
        while time.time() < deadline:
            try:
                incoming = inbox.get(timeout=0.1)
            except queue.Empty:
                emit({"type": "stream_event", "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "."}}, "parent_tool_use_id": None})
                continue
            if incoming is None:
                sys.exit(0)
            if incoming.get("type") == "control_request":
                handle_control(incoming)
                if incoming.get("request", {}).get("subtype") == "interrupt":
                    interrupted()
                    return
            elif incoming.get("type") == "user":
                steered.append(text_of(incoming))
                if word == "steer":
                    break
        assistant([{"type": "text", "text": "steered:" + ",".join(steered) if steered else "finished"}])
        result()
    elif word == "crash":
        emit({"type": "stream_event", "event": {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "partial"}}, "parent_tool_use_id": None})
        sys.stderr.write("fake claude crashed on purpose\n")
        sys.stderr.flush()
        sys.exit(3)
    elif word == "think":
        assistant([{"type": "thinking", "thinking": "let me think"}, {"type": "text", "text": "Looking."}, {"type": "tool_use", "id": "toolu_t", "name": "Read", "input": {"file_path": "a"}}])
        emit({"type": "user", "message": {"role": "user", "content": [{"tool_use_id": "toolu_t", "type": "tool_result", "content": "contents"}]}, "parent_tool_use_id": None})
        assistant([{"type": "text", "text": "Done thinking."}])
        result()
    elif word == "fail":
        result(is_error=True, text="Something went wrong")
    elif word == "big":
        assistant([{"type": "text", "text": "x" * 300000}])
        result()
    elif word == "bye":
        sys.exit(0)
    else:
        assistant([{"type": "text", "text": "ok"}])
        result()


while True:
    message = inbox.get()
    if message is None:
        break
    if message.get("type") == "control_request":
        handle_control(message)
        if message.get("request", {}).get("subtype") == "interrupt":
            pass
        continue
    if message.get("type") == "user":
        turn(message)
