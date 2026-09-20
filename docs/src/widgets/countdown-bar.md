# CountdownBar

`src/components/countdown_bar.rs`

## Purpose

A thin bar that shrinks as a countdown runs out. It is drawn under a notification toast to show how much of the toast's lifetime is left, and it drives its own repaints so nothing has to tick the application to keep it moving.

## Why Not a Container?

The obvious version of this is a `container` whose width is a fraction of the total, recomputed on every update. That works, but nothing in the application state changes between the toast arriving and its deadline, so the fraction only moves if something forces repaints. That means an `every(..)` subscription running for as long as a toast is on screen, purely to redraw.

A widget can ask for the next frame itself, the same way `Slide` and `Collapsible` do:

```rust
if let event::Event::Window(window::Event::RedrawRequested(now)) = event {
    shell.request_redraw();
}
```

So `CountdownBar` takes the deadline, resolves the fraction at draw time, and re-requests a frame while it still has something to show. No subscription, no application-level tick, and the bar stops asking for frames the moment it is paused or empty.

## API

```rust
pub enum Countdown {
    Running { deadline: Instant, total: Duration },
    Paused { remaining: Duration, total: Duration },
}

pub fn countdown_bar(countdown: Countdown, height: f32, color: Color) -> CountdownBar;

impl CountdownBar {
    // When off, the bar steps every 250ms instead of every frame
    pub fn animated(self, animated: bool) -> Self;
}
```

## Notes

- The widget is stateless. Everything it draws comes from the `Countdown` it is handed on each view pass, so a toast being replaced or paused needs no widget-side bookkeeping.
- `Paused` holds its width and requests no further frames. The notifications module pauses the whole toast stack while the pointer is over it, so this is the resting state whenever a toast is being read.
- `animated(false)` follows the `animations.enabled` config. The bar still counts down, in 250ms steps, rather than disappearing when animations are off.
- An empty bar draws nothing rather than a zero-width quad, and stops requesting frames.

## Usage

```rust
// In modules/notifications.rs, under the toast card
countdown_bar(countdown, COUNTDOWN_BAR_HEIGHT, color)
    .animated(self.animations_enabled)
```

The bar is placed outside the toast card's clipped container, so a notification body long enough to overflow `toast_max_height` cannot cut it off.
