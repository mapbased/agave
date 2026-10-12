# Repository guidance for coding agents

## Cargo package names

This workspace contains both `agave-*` and `solana-*` packages. Do not infer a
package name from its directory. The authoritative Cargo package name is the
`name` field in the crate's `[package]` table.

Before running a package-scoped Cargo command, verify the name in the manifest.

## Conversation Logging in Git

All conversations with the user must be documented in full text within the Git repository (specifically stored in the `runner` sub-repository):
- **Directory**: `runner/chat/`
- **File Naming**: `YYYY-MM-DD-<topic>.md` (e.g. `2026-10-12-对话记录保存机制配置.md`)
- **Content Requirement**: Full text record of every Q&A turn between User and Agent (preserving prompts, context, and complete responses).
- **Workflow**: For each active conversation session, update/append to the corresponding Markdown file in `runner/chat/` so all discussions are tracked in Git under the `runner` repository.

