#!/usr/bin/env python3
"""
Export conversation transcripts from Antigravity IDE storage to markdown files in git repository.

Usage:
    python3 scripts/export_conversations.py --all
    python3 scripts/export_conversations.py --current
    python3 scripts/export_conversations.py <conversation-id>
"""

import os
import sys
import json
import re
import argparse
from datetime import datetime
from pathlib import Path

DEFAULT_OUTPUT_DIR = Path(__file__).resolve().parent.parent / "runner" / "chat"
BRAIN_DIR = Path(os.path.expanduser("~/.gemini/antigravity-ide/brain"))
CURRENT_CONV_ID = "1f43c9df-97ad-49c3-b0e8-e4afa98382e8"

# Known conversation metadata
KNOWN_CONVERSATIONS = {
    "1f43c9df-97ad-49c3-b0e8-e4afa98382e8": {
        "title": "配置在Git中保存完整对话记录机制",
        "date": "2026-10-12"
    },
    "0e426049-fc3a-479b-8da1-353c695d7388": {
        "title": "分析模拟交易失败原因",
        "date": "2026-10-11"
    },
    "54a465a4-66e2-48d7-af10-a844b7ef4b37": {
        "title": "恢复系统崩溃前任务",
        "date": "2026-10-10"
    },
    "cae4e2f0-4fd4-43d0-bc62-e681c8aa0a73": {
        "title": "交易系统可视化设计方案",
        "date": "2026-10-09"
    }
}

def clean_user_input(text: str) -> str:
    m = re.search(r"<USER_REQUEST>\s*(.*?)\s*</USER_REQUEST>", text, re.DOTALL)
    if m:
        return m.group(1).strip()
    return text.strip()

def sanitize_filename(name: str) -> str:
    s = re.sub(r'[\\/*?:"<>| ]+', '_', name).strip('_')
    return s[:60]

def parse_conversation(conv_id: str):
    conv_dir = BRAIN_DIR / conv_id
    if not conv_dir.exists():
        print(f"Conversation directory not found: {conv_dir}", file=sys.stderr)
        return None

    full_log = conv_dir / ".system_generated" / "logs" / "transcript_full.jsonl"
    normal_log = conv_dir / ".system_generated" / "logs" / "transcript.jsonl"

    log_file = full_log if full_log.exists() else normal_log
    if not log_file.exists():
        print(f"Log file not found for {conv_id}", file=sys.stderr)
        return None

    turns = []
    current_user = None

    with open(log_file, "r", encoding="utf-8", errors="replace") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                data = json.loads(line)
            except Exception:
                continue

            stype = data.get("type")
            created_at = data.get("created_at", "")

            if stype == "USER_INPUT":
                raw = data.get("content", "")
                user_text = clean_user_input(raw)
                # If there was a previous user input without response, save it
                if current_user:
                    turns.append((current_user, ("", "")))
                current_user = (created_at, user_text)

            elif stype == "PLANNER_RESPONSE":
                content = data.get("content", "")
                tool_calls = data.get("tool_calls", [])
                # Only treat as text response if there is content and no tool calls,
                # or if content is meaningful text
                if content and not tool_calls and current_user:
                    turns.append((current_user, (created_at, content)))
                    current_user = None

    if current_user:
        turns.append((current_user, ("", "")))

    return turns

def export_conversation_to_md(conv_id: str, output_dir: Path):
    output_dir.mkdir(parents=True, exist_ok=True)
    meta = KNOWN_CONVERSATIONS.get(conv_id, {})
    title = meta.get("title", f"会话_{conv_id[:8]}")
    date_str = meta.get("date", datetime.now().strftime("%Y-%m-%d"))

    turns = parse_conversation(conv_id)
    if not turns:
        print(f"No conversation turns found for {conv_id}")
        return None

    filename = f"{date_str}-{sanitize_filename(title)}.md"
    target_path = output_dir / filename

    lines = []
    lines.append(f"# 对话记录: {title}")
    lines.append("")
    lines.append(f"- **日期**: {date_str}")
    lines.append(f"- **会话 ID**: `{conv_id}`")
    lines.append(f"- **导出时间**: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    lines.append("")
    lines.append("---")
    lines.append("")

    for i, ((u_time, u_text), (a_time, a_text)) in enumerate(turns, start=1):
        lines.append(f"## 第 {i} 轮问答")
        lines.append("")
        time_tag = f" ({u_time})" if u_time else ""
        lines.append(f"### 👤 用户{time_tag}")
        lines.append("")
        lines.append(u_text if u_text else "*(空)*")
        lines.append("")

        if a_text:
            a_time_tag = f" ({a_time})" if a_time else ""
            lines.append(f"### 🤖 Antigravity{a_time_tag}")
            lines.append("")
            lines.append(a_text)
            lines.append("")
        lines.append("---")
        lines.append("")

    with open(target_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    print(f"Exported {len(turns)} turns -> {target_path}")
    return target_path

def main():
    parser = argparse.ArgumentParser(description="Export conversations to Git docs")
    parser.add_argument("conversation_id", nargs="?", help="Specific conversation ID to export")
    parser.add_argument("--all", action="store_true", help="Export all known conversations")
    parser.add_argument("--current", action="store_true", help="Export current conversation")
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR, help="Target directory")

    args = parser.parse_args()

    if args.all:
        for cid in KNOWN_CONVERSATIONS:
            export_conversation_to_md(cid, args.output_dir)
    elif args.current:
        export_conversation_to_md(CURRENT_CONV_ID, args.output_dir)
    elif args.conversation_id:
        export_conversation_to_md(args.conversation_id, args.output_dir)
    else:
        # Default: export current
        export_conversation_to_md(CURRENT_CONV_ID, args.output_dir)

if __name__ == "__main__":
    main()
