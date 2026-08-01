# HyperTalk Reference

> The language HyperLab speaks, as it stands today.
>
> This document describes what is *implemented*, not what is planned. Anything
> missing is listed at the bottom rather than left for you to discover.

HyperTalk reads like English on purpose. `put "Hello" into field "Greeting"`
means what it says, and that is the whole idea: the person writing the script
is usually the person using the stack.

---

## Scripts and handlers

A script is a list of handlers and nothing else. There is no top-level code.

```hypertalk
on mouseUp
  answer "Hello"
end mouseUp

function double n
  return n * 2
end double
```

- `on <name>` handles a message.
- `function <name>` computes a value, and is called as `double(21)`.
- Parameters are separated by commas: `on greet first, last`.
- Names are case-insensitive: `on MouseUp` answers to `mouseUp`.
- The `end` must name the same handler.

Comments start with `--` and run to the end of the line. A line ending in `\`
(or the classic `¬`) continues onto the next.

---

## Messages, and the path they travel

Clicking a button sends `mouseUp` to that button. If the button has no
`mouseUp` handler, the message carries on:

```
button → card → background → stack
```

The first handler found runs. If nobody handles the message, nothing happens —
that is normal, not an error.

| Message | Sent when |
| --- | --- |
| `mouseDown`, `mouseUp` | A part is clicked |
| `mouseEnter`, `mouseLeave` | The pointer arrives or leaves |
| `openCard`, `closeCard` | Navigation |
| `openStack`, `closeStack` | A stack is opened or closed |
| `keyDown` | A key is pressed |
| `fieldChanged` | A field's contents are edited |
| `idle` | Nothing is happening |

Inside a handler:

- `me` is the object whose script is running.
- `the target` is the object the message was originally sent to.
- `pass mouseUp` hands the message on to the next object in the path, after
  doing whatever you did first.
- `send "mouseUp" to button 1` starts a message somewhere else entirely.

Any command HyperLab does not recognise is sent as a message too, so a handler
named `greet` on the stack is called from anywhere with `greet "world"`.

---

## Containers

Anything that holds text is a container: a variable, `it`, a field, the
message box.

```hypertalk
put 1 into total            -- a variable
put "Ada" into field "Name" -- a field
put total                   -- the message box
get 2 + 2                   -- puts the answer into `it`
```

`put` also inserts without replacing:

```hypertalk
put "Dr " before field "Name"
put "!" after field "Name"
```

Variables are local to the handler they appear in. A name that has never been
used stands for itself, so `put cancel into it` needs no quotes.

```hypertalk
global counter    -- this handler now shares `counter` with every other
add 1 to counter  -- handler that also declares it
```

---

## Chunks

Chunks are what make text feel like a data structure, and they are how most
real stacks store lists.

```hypertalk
put word 2 of field "Notes" into secondWord
put char 1 to 3 of "abcdef"         -- "abc"
put item 3 of "a,b,c"               -- "c"
put line 2 of field "Items"         -- the second line
```

They nest, innermost last:

```hypertalk
put "D" into word 2 of line 2 of field "Notes"
```

And they can be written to, including past the end, which is how lists grow:

```hypertalk
put "c" into item 3 of "a,b"        -- "a,b,c"
```

Units: `char`, `word`, `item` (comma-separated), `line`. Counting starts at
one. A range that runs off the end is clamped.

---

## Objects

```hypertalk
card 3                     card "Home"            card id 12
first card                 last card              next card
this card                  this background        this stack
button "Go"                field 2                btn 1
card field "Name"          background button "Next"
field "Name" of card 2
```

- A **quoted** specifier is always a name: `field "3"` is not `field 3`.
- Without `card` or `background`, a part is looked for on the card first and
  then on its background — the rule that lets one card override a shared
  layout.
- A specifier binds tightly. `field "Name" & "!"` is `(field "Name") & "!"`;
  for anything more elaborate use brackets: `field ("Row " & n)`.
- `there is a card "Home"` and `there is no field "Notes"` test existence.

---

## Properties

```hypertalk
set the visible of button "Go" to false
set the width of me to 120
put the name of the target into field "Log"
```

Every object has `id`, `name` and `script`. Parts also have `left`, `top`,
`width`, `height`, `right`, `bottom`, `rect`, `visible`, `enabled` and
`style`; fields add `text`, `locked` and `wrap`; buttons add `showName`,
`hilite` and `autoHilite`. `the number of` a card or part is its position
among its siblings, and `the owner of` names what it belongs to.

Properties are open-ended: setting one HyperLab has never heard of stores it,
saves it, and shows it in the inspector. This is how themes, plugins and
future versions add behaviour without changing the object model.

---

## Control flow

```hypertalk
if x > 10 then answer "big"

if x > 10 then
  answer "big"
else if x > 3 then
  answer "middling"
else
  answer "small"
end if
```

```hypertalk
repeat 3 times          … end repeat
repeat forever          … end repeat
repeat while x < 10     … end repeat
repeat until x = 10     … end repeat
repeat with i = 1 to 10 … end repeat
repeat with i = 10 down to 1 … end repeat
```

`exit repeat` leaves a loop, `next repeat` starts its next turn, `exit
<handler>` leaves the handler, and `exit to HyperLab` abandons everything.

A loop that runs more than a million times is stopped with an error. A live
programming system must not be able to freeze itself.

---

## Operators

Loosest to tightest:

| Operators | |
| --- | --- |
| `or` | |
| `and` | |
| `not` | prefix |
| `=` `is` `<>` `is not` `<` `>` `<=` `>=` `contains` `is in` `starts with` `ends with` | |
| `&` `&&` | text, `&&` inserts a space |
| `+` `-` | |
| `*` `/` `div` `mod` | |
| `^` | right-associative |
| `-` `not` | prefix |

Comparison is case-insensitive, and numeric when both sides look like numbers,
so `"7" = 7` is true and `"Ada" = "ADA"` is true.

Constants: `empty`, `true`, `false`, `quote`, `return`, `space`, `tab`,
`comma`, `colon`, `pi`, `linefeed`, `newline`.

---

## Commands

| Command | |
| --- | --- |
| `put … [into\|before\|after] <container>` | Store text |
| `set [the] <property> [of <object>] to <value>` | Change a property |
| `get <expression>` | Put it into `it` |
| `add`/`subtract`/`multiply`/`divide` | Arithmetic in place |
| `go [to] <card>`, `go back` | Navigate |
| `send <message> to <object>` | Send a message elsewhere |
| `answer <text>` | Show a message |
| `ask <question> [with <default>]` | Ask for a line of text; the answer lands in `it` |
| `ask assistant <request>` | Ask a language model to do something; its reply lands in `it` |
| `beep` | Make a noise |
| `wait <n> [ticks\|seconds]` | Pause |
| `hide`/`show <object>` | Set `visible` |
| `pass`, `return`, `exit`, `global` | Control |

`answer` and `ask` are modal: the script stops until the dialog is dismissed.
Cancelling `ask` leaves `it` empty and sets `the result` to `"Cancel"`, so a
handler can tell the difference between an empty answer and no answer:

```hypertalk
ask "What needs doing?" with ""
if the result is "Cancel" then exit mouseUp
put it & return after field "Items"
```

A caller with no window — a test, or an MCP tool — has no way to answer, so
every question is cancelled. A script that checks `the result` therefore works
unattended as well as on screen.

`choose`, `doMenu`, `play`, `visual`, `lock` and `unlock` are accepted and do
nothing, so scripts written for HyperCard do not fall over on a line that only
affects appearance.

A handler always wins over a built-in: write `on beep` and `beep` is yours.

---

## Functions

`length`, `abs`, `sqrt`, `trunc`, `round`, `exp`, `ln`, `sin`, `cos`, `tan`,
`min`, `max`, `sum`, `average`, `random`, `charToNum`, `numToChar`, `offset`,
`value`, `ai`.

They can be written either way:

```hypertalk
put length(field "Notes") into n
put the length of field "Notes" into n
```

Counting:

```hypertalk
the number of cards
the number of backgrounds
the number of buttons        -- of this card and its background
the number of card fields
the number of words of field "Notes"
```

Without arguments: `the date`, `the long date`, `the abbreviated date`,
`the time`, `the long time`, `the seconds`, `the ticks`, `the result`.

`the value of "2 + 2"` evaluates arithmetic in text. It deliberately
understands nothing else: it is constant folding, not a second way into the
runtime.

---

## Asking a language model

Two things: one is a value, the other is a request.

`ai(…)` is a function. It answers in words and touches nothing, so it goes
wherever any other value goes:

```hypertalk
put ai("Summarize this card") into field "Summary"

if ai("Is this address in Portugal?") is "yes" then
  put "EU" into field "Region"
end if
```

`ask assistant` is a command. It asks for something to be *done*, not merely
described:

```hypertalk
ask assistant "Add a search button to this card"
put it into field "Notes"
```

The reply lands in `it`, exactly as with `ask`. And exactly as with `ask`,
the failure is a value rather than a stop: if no provider is configured, `it`
is empty and `the result` says why, so a stack that uses the assistant still
runs on a machine that has none.

```hypertalk
ask assistant "Tidy up this card"
if the result is not empty then answer "No assistant: " & the result
```

`ai(…)` cannot do that, because it sits in the middle of an expression and
there is no honest value for it to be. A refused `ai(…)` stops the handler
with the reason.

Both are ordinary names, so a handler of your own wins:

```hypertalk
function ai question
  return "not today"
end ai
```

### What a script can and cannot get done

From a script, both of these answer in words. Neither can restructure the
stack, and that is a deliberate limit rather than an unfinished one: your
handler is *already running* inside the runtime, and an assistant rearranging
cards between two of your statements would pull the ground out from under it.
Changes belong in the AI sidebar, where nothing is mid-handler and every edit
goes through the same undoable commands you use.

Nothing is sent but your own words. HyperLab adds no description of the stack
to `ai(…)` — it does not need to guess, because you say what to include:

```hypertalk
put ai("Summarize this, in one line: " & field "Notes") into field "Summary"
```

The runtime does not know what a prompt looks like. It passes your words
through and hands back the reply.

---

## Reserved words

`char`, `chars`, `character`, `characters`, `word`, `words`, `item`, `items`,
`line` and `lines` introduce chunks, and `card`, `cd`, `background`, `bg`,
`bkgnd`, `stack`, `button`, `btn`, `field`, `fld`, `me` and `this` introduce
objects. None of them can be used as a variable name.

`assistant` is reserved only where it follows `ask`, so `ask assistant "…"`
is one command rather than `ask` applied to a variable. Everywhere else it is
an ordinary name.

---

## Not implemented yet

Named so that you find out here rather than at the wrong moment:

- **`is a number`, `is a date`** and the other type tests.
- **`find`, `sort`, `the selection`, `visual effect`, `do`,
  `the clickLoc`,** and painting of any kind.
