# Writing a game

Everything so far has been programs that read something, work something out, and
print an answer. A game is different in one way that changes the shape of the
whole program: **it keeps going while you are not doing anything.**

A ball falls whether or not you touch the keyboard. A snake keeps walking. That
is the thing to build, and it takes four pieces.

## The loop

```
while playing {
  handle_input()      // if anything was pressed
  advance()           // whether or not it was
  draw()              // the whole picture, at once
  sleep(50)           // then wait, so it runs at a chosen speed
}
```

Read that order carefully. **Input is first and optional; advancing is
unconditional.** Almost every mistake in a first game comes from tying the
second to the first.

## Why `read_key()` on its own is not enough

The obvious way to read a key is `read_key()`, and it waits. That is right for a
guessing game and wrong here, because while your program is waiting it is not
running: nothing falls, nothing walks, and the picture on the screen sits still
until somebody presses something.

`key_ready()` asks whether a read would answer immediately, so you can look
without committing to a wait:

```
if key_ready() {
  let pressed = read_key()
  ...
}
```

Now the loop goes round whether or not anything was pressed, and `advance()`
happens either way. That is the whole difference.

### One key per turn, or all of them?

Both are right, for different games.

```
if key_ready() { ... }        // one per turn
while key_ready() { ... }     // everything pressed since the last picture
```

**Snake wants one.** It turns once per step, so three keys taken in one turn
would throw two away — and worse, turning up and then left between two steps
walks you into your own neck without that ever being drawn.

**A typing game wants all of them**, because every press is worth a point and
none should be lost.

### Stopping

`key_ready()` answers `true` when the keys have run out, which sounds wrong and
is what lets a loop finish. It is telling you the read will not make you wait —
and at the end it does not, it gives `nil` at once:

```
if key_ready() {
  let pressed = read_key()
  if pressed == nil {          // no more input, ever
    playing = false
  }
}
```

**Always check for `"ctrl+c"` too.** While a program is reading keys the terminal
hands Control-C over as an ordinary key instead of stopping anything, so a game
that ignores it cannot be stopped from the keyboard.

## Drawing

`print` puts a line below the last one, so a picture printed every turn gives
you a column of pictures scrolling upwards. `clear()` empties the screen and
puts the cursor back at the top left, so the next picture lands where the last
one was and the thing appears to move.

**Build the whole picture as one string and print it once.**

```
fn draw(things) {
  let out = ""
  for y in range(0, height) {
    for x in range(0, width) {
      out = out + character_at(things, x, y)
    }
    out = out + "\n"
  }
  print(out)
}
```

Printing it row by row works, and you can see it working: the terminal draws
each row as it arrives, and your eye catches the sweep down the screen.

`hide_cursor()` is worth calling once at the start. Otherwise the cursor blinks
wherever your last character landed, which in a redrawn picture is the bottom
right. You do not have to put it back — it returns when your program ends,
however it ends.

## Speed

`sleep(ms)` is what makes the difference between a game and a blur. Without it a
loop runs as fast as the machine allows, which is millions of turns a second: a
processor at full tilt and a picture nobody can see.

```
sleep(50)      // about twenty pictures a second
sleep(100)     // ten, which suits anything on a grid
```

Start around 100 for a grid game. Anything below about 30 is faster than most
people can react to.

## A whole one, small

```
let width = 20
let x = 10
let playing = true

hide_cursor()

while playing {
  if key_ready() {
    let pressed = read_key()
    if pressed == nil || pressed == "q" || pressed == "ctrl+c" {
      playing = false
      break
    }
    if pressed == "left" && x > 0 {
      x = x - 1
    } else if pressed == "right" && x < width - 1 {
      x = x + 1
    }
  }

  let row = ""
  for i in range(0, width) {
    if i == x {
      row = row + "#"
    } else {
      row = row + "."
    }
  }

  clear()
  print(row)
  print("left and right move, q quits")
  sleep(80)
}

show_cursor()
```

## Four to read

- **[life.miru](../examples/life.miru)** — no input at all, so it is nothing but
  the drawing half. Start here.
- **[snake.miru](../examples/snake.miru)** — the whole shape, including growing
  an array at the front.
- **[pong.miru](../examples/pong.miru)** — a ball that never stops, which is the
  clearest demonstration of why `key_ready()` exists.
- **[tetris.miru](../examples/tetris.miru)** — a board it both reads and writes,
  which is the shape most games with a grid actually have. Read it for two
  things. Turning a piece is not trigonometry: on a grid a quarter turn sends
  the cell at `(x, y)` to `(box - 1 - y, x)`, and that is the whole of it.
  Clearing a full row is array work — keep the rows with a gap, build as many
  empty rows as went away, and join the two with `+`.

## Three things that will catch you

**Your game will not run in the playground.** A page cannot pause: it draws
between one piece of work and the next, so a program that waited would freeze
the tab instead of animating. `sleep`, `key_ready`, and `clear` all refuse
there. Games are for a terminal.

**Do not call `term_size()` in a game you want to test.** It fails when the
output is not a screen, which is exactly the situation when you pipe a program's
output somewhere to check it. All four examples pick their own size for that
reason, and it is why you can pipe keys into snake and compare what it drew.

**A game that draws random numbers cannot be checked unless you can fix the
seed.** `seed(n)` gives the same stream every time, so a test can say what the
game must do; `seed(now())` gives a different game every run, which is what a
player wants. Tetris takes the seed from the command line and falls back to the
clock, so it is both:

```
miru run examples/tetris.miru 7
```

The same pieces arrive in the same order every time you run that.

---
Previous: [Handling errors](16-handling-errors.md) | Next: [Next steps](18-next-steps.md)
