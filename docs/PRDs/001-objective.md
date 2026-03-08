# Objective

> This document covers only the minimal objective. Detailed requirements, design decisions, and implementation specifics are intentionally excluded.

## LLM Providers

Extensible through a common LLM provider interface, with each provider implementing its own adapter.

- Anthropic (default)
- OpenAI

## Core Concepts

- **Workspace**: A directory rooted at a specific path, typically a cloned GitHub repository.
- **Room**: A topic-based conversation room within a workspace (e.g., `implement` for hands-on coding, `review` for reviewing others' code). Each room has a **leader** that receives user input, either handles it directly or delegates tasks to appropriate agents in the room, aggregates the results, and responds to the user.
- **Agent**: A task executor within a room, assigned work by the room's leader.

## Architecture

Client-server model running locally on a single machine. No authentication required.

- The server runs persistently, so agents keep working even when all clients are disconnected.
- Multiple clients can run simultaneously, each viewing and interacting with different rooms.
- When multiple clients view the same room, content is synced in real time.

## Interface

TUI-based client, designed to be fully usable over SSH for remote work.

## Notifications

When the user is not focused on an in-progress task, send notifications via user-selected channels (e.g., sound, Telegram, Slack). Notifications are extensible through a common notification interface, with each channel implemented as a separate provider.

## Skills

Inspired by Claude Code's skill management. Skills are installable from a marketplace using the `<owner>/<repo>` format. A marketplace contains **plugins**, and each plugin contains one or more **skills**.

## MCP

Support the Model Context Protocol (MCP) to allow agents to interact with external tools and data sources.
