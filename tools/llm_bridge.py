#!/usr/bin/env python3
"""
TuniCore LLM Bridge - inline relay mode.

Sits between user terminal and QEMU serial socket.
Intercepts LLM queries (STX+"LLM:..."+ETX) and injects AI responses.

Usage:
  export GEMINI_API_KEY="your-key"
  python3 tools/llm_bridge.py

  # Then in another terminal, boot QEMU with:
  # -chardev socket,id=ser0,path=/tmp/tc_serial.sock,server=on,wait=off
  # -serial chardev:ser0
"""

import os
import sys
import socket
import select
import json
import urllib.request

SOCKET_PATH = "/tmp/tc_serial.sock"
STX = 0x02
ETX = 0x03

SYSTEM_PROMPT = """You are TuniCore AI, built into a bare-metal x86_64 OS written in Rust.
Keep responses under 3 sentences. Respond in the same language as the question."""


def call_gemini(prompt, api_key):
    url = f"https://generativelanguage.googleapis.com/v1beta/models/gemini-2.0-flash:generateContent?key={api_key}"
    payload = {
        "contents": [{"parts": [{"text": f"{SYSTEM_PROMPT}\n\nUser: {prompt}"}]}],
        "generationConfig": {"maxOutputTokens": 200, "temperature": 0.7}
    }
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            result = json.loads(resp.read())
            text = result["candidates"][0]["content"]["parts"][0]["text"]
            return text.replace('\r', '').replace('\n', ' ').strip()[:900]
    except Exception as e:
        return f"[AI error: {e}]"


def call_openai(prompt, api_key):
    url = "https://api.openai.com/v1/chat/completions"
    payload = {
        "model": "gpt-4o-mini",
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": prompt}
        ],
        "max_tokens": 200
    }
    data = json.dumps(payload).encode()
    req = urllib.request.Request(url, data=data, headers={
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    })
    try:
        with urllib.request.urlopen(req, timeout=15) as resp:
            result = json.loads(resp.read())
            return result["choices"][0]["message"]["content"].replace('\r', '').replace('\n', ' ').strip()[:900]
    except Exception as e:
        return f"[AI error: {e}]"


def get_ai_response(prompt):
    gemini_key = os.environ.get("GEMINI_API_KEY")
    openai_key = os.environ.get("OPENAI_API_KEY")
    if gemini_key:
        return call_gemini(prompt, gemini_key)
    elif openai_key:
        return call_openai(prompt, openai_key)
    else:
        return f"[Echo] You asked: {prompt}"


def main():
    print("╔══════════════════════════════════════╗")
    print("║   TuniCore LLM Bridge v1.0           ║")
    print("╚══════════════════════════════════════╝")

    provider = "Gemini" if os.environ.get("GEMINI_API_KEY") else \
               "OpenAI" if os.environ.get("OPENAI_API_KEY") else "Echo (no key)"
    print(f"  Provider: {provider}")
    print(f"  Socket:   {SOCKET_PATH}")

    # Connect to QEMU serial socket
    sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        sock.connect(SOCKET_PATH)
    except Exception as e:
        print(f"  ERROR: Cannot connect: {e}")
        print(f"  Start QEMU first with: -chardev socket,id=ser0,path={SOCKET_PATH},server=on,wait=off")
        sys.exit(1)

    print(f"  OK: Connected!")
    print(f"  Type commands or Ctrl+C to quit\n")

    sock.setblocking(False)
    stdin_fd = sys.stdin.fileno()

    buf = bytearray()
    in_query = False
    line_buf = b""

    try:
        while True:
            readable, _, _ = select.select([sock, stdin_fd], [], [], 0.05)

            for r in readable:
                if r == sock:
                    try:
                        data = sock.recv(4096)
                    except:
                        data = b""
                    if not data:
                        print("\n  Connection closed.")
                        return

                    for byte in data:
                        if byte == STX:
                            buf.clear()
                            in_query = True
                            continue

                        if byte == ETX and in_query:
                            in_query = False
                            msg = buf.decode('utf-8', errors='replace')

                            if msg.startswith("LLM:"):
                                query = msg[4:]
                                print(f"\n  < AI Query: {query}")
                                response = get_ai_response(query)
                                print(f"  > AI Reply: {response[:80]}...")

                                frame = bytes([STX]) + b"RSP:" + response.encode('utf-8', errors='replace') + bytes([ETX])
                                sock.sendall(frame)
                            buf.clear()
                            continue

                        if in_query:
                            buf.append(byte)
                        else:
                            sys.stdout.buffer.write(bytes([byte]))
                            sys.stdout.buffer.flush()

                elif r == stdin_fd:
                    ch = os.read(stdin_fd, 1)
                    if ch:
                        sock.sendall(ch)

    except KeyboardInterrupt:
        print("\n  Bridge stopped.")
    finally:
        sock.close()


if __name__ == "__main__":
    main()
