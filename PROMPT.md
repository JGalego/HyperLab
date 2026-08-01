# HyperLab

## Project Overview

You are my principal software engineer and technical architect.

We are building **HyperLab**, an open-source, AI-native successor inspired by Apple's HyperCard.

This is **not** a clone. It is a modern reinterpretation that preserves the philosophy of HyperCard—cards, stacks, fields, buttons, scripts, and end-user programming—while adding first-class AI and LLM capabilities.

The project should feel as revolutionary today as HyperCard felt in 1987.

---

# Core Vision

HyperLab should make it possible for anyone to build desktop applications by combining:

- visual cards
- natural language
- scripting
- AI

The UI should feel instantly familiar to anyone who used HyperCard while remaining modern, polished, fast, and extensible.

AI is not an add-on.

AI is part of the programming model.

---

# Design Principles

1. Simplicity over cleverness.
2. Everything is inspectable.
3. Everything is scriptable.
4. Everything can be modified live.
5. Local-first.
6. AI should assist—not replace—the user.
7. Cross-platform.
8. Open source from day one.
9. Clean architecture.
10. No unnecessary dependencies.

---

# Technology Stack

Use:

- Rust
- Tauri
- React + TypeScript
- Vite

The Rust backend should contain:

- runtime
- parser
- stack engine
- persistence
- AI bridge
- MCP integration

The frontend should only be responsible for rendering and user interaction.

Keep clear separation between UI and runtime.

---

# Repository Layout

```
hyperlab/

    README.md

    apps/
        desktop/

    crates/
        runtime/
        parser/
        stack/
        persistence/
        ai/
        mcp/

    docs/
        architecture.md
        roadmap.md
        hypertalk.md

    examples/
        Address Book
        Recipe Box
        Todo
```

---

# MVP

Version 0.1 should support:

- create stack
- create cards
- create buttons
- create fields
- save stacks
- load stacks
- card navigation
- object inspector
- property editor
- script editor

No AI is required for the first milestone.

The runtime should be clean enough that AI can later integrate naturally.

---

# Object Model

Everything derives from objects.

```
Stack
    Backgrounds
    Cards

Card
    Fields
    Buttons

Field

Button
```

Every object has:

- id
- name
- script
- properties
- geometry

---

# Message Passing

The runtime should use HyperCard-style messaging.

Example:

```
mouseDown

mouseUp

openCard

closeCard

openStack

closeStack

keyDown
```

The architecture should make adding future events trivial.

---

# HyperTalk

Create a modern HyperTalk parser.

Example:

```
on mouseUp

    answer "Hello"

end mouseUp
```

Support:

- handlers
- variables
- repeat
- if
- functions
- messages
- object references

The parser should produce an AST.

The runtime should execute the AST.

---

# Renderer

The initial renderer should emulate the classic monochrome HyperCard aesthetic.

Requirements:

- crisp pixel-perfect controls
- monochrome UI
- rounded buttons
- Chicago-inspired typography (using an open licensed alternative)
- grayscale icons
- card drop shadows
- authentic spacing

Do NOT copy Apple's copyrighted assets.

Instead, create an original "Neo Classic" design inspired by the era.

Future themes can include:

- Dark
- Color
- Retro Macintosh
- Modern Minimal

---

# Persistence

Design a future-proof stack format.

Prefer JSON initially.

Example:

```
stack.json

cards/

backgrounds/

assets/

scripts/
```

Eventually support binary serialization.

---

# AI Architecture

The AI system should be provider-independent.

Create interfaces for:

```
OpenAI

Anthropic

Google

Ollama

OpenRouter

Local models
```

Do not hardcode any provider.

---

# MCP

Design AI around the Model Context Protocol (MCP).

HyperLab itself should expose tools.

Examples:

```
currentCard

readField

writeField

createCard

createButton

runScript

findCards

searchStacks
```

Later HyperLab should also consume external MCP servers.

---

# AI Language Extensions

Eventually HyperTalk should support syntax like:

```
put ai("Summarize this card") into field "Summary"

ask assistant
    "Generate five cards"

put ai(field "Notes") into field "Summary"
```

Do not implement this yet.

Instead, ensure the architecture supports future language extensions.

---

# Inspector

Every object should be inspectable.

Show:

- properties
- script
- geometry
- hierarchy

Eventually:

- AI assistant
- execution history
- messages
- debugging

---

# AI Sidebar

Future feature.

The sidebar can answer:

> Explain this script.

> Refactor this.

> Add a search button.

> Make this prettier.

The AI should edit the stack through runtime APIs—not by manipulating UI state directly.

---

# Architecture Rules

Never let UI directly manipulate stack data.

The flow should always be:

```
UI

↓

Commands

↓

Runtime

↓

Model

↓

Renderer
```

This makes scripting, undo, collaboration, automation, testing, and AI all use the same execution path.

---

# Coding Standards

- Idiomatic Rust
- Idiomatic React
- Strict TypeScript
- No giant files
- Small modules
- Extensive documentation
- Unit tests
- Integration tests
- Clean naming
- Minimal dependencies

Avoid premature optimization.

Favor readability.

---

# Documentation

Every public module should include documentation.

Maintain:

- Architecture
- Roadmap
- Contributing
- HyperTalk Reference

---

# GitHub

Initialize:

- MIT License
- Code of Conduct
- Contributing Guide
- GitHub Actions
- CI
- Formatting
- Clippy
- Rustfmt
- Prettier
- ESLint

---

# Roadmap

## Phase 1

Core runtime.

## Phase 2

Desktop editor.

## Phase 3

HyperTalk.

## Phase 4

AI sidebar.

## Phase 5

MCP.

## Phase 6

AI-native HyperTalk.

## Phase 7

Plugins.

## Phase 8

Collaborative stacks.

---

# Long-Term Vision

HyperLab should become what HyperCard might have evolved into if it had continued to evolve for another thirty years.

It should feel like:

- HyperCard
- Visual Basic
- Smalltalk
- Live programming
- Modern AI assistants
- MCP
- Local-first computing

combined into a single coherent platform.

The codebase should be designed so that developers fifty years from now could still understand it.

Whenever making implementation decisions, optimize for:

- simplicity
- elegance
- extensibility
- maintainability

over cleverness.

Build this as if it will become the reference implementation for AI-native visual programming.