use iced::{
    Background, Border, Color, Length, Rectangle, Size,
    core::{
        Clipboard, Layout, Shell, Widget, event, layout, mouse, renderer, widget::Tree, window,
    },
};
use std::time::{Duration, Instant};

type Element<'a, Message, Theme, Renderer> = iced::core::Element<'a, Message, Theme, Renderer>;

/// How far a countdown has left to run, resolved at draw time.
#[derive(Debug, Clone, Copy)]
pub enum Countdown {
    /// Counting down towards `deadline`.
    Running { deadline: Instant, total: Duration },
    /// Held with `remaining` left to run.
    Paused {
        remaining: Duration,
        total: Duration,
    },
}

impl Countdown {
    /// The share of the lifetime still to run, in `0.0..=1.0`.
    fn fraction(&self, now: Instant) -> f32 {
        let (remaining, total) = match *self {
            Self::Running { deadline, total } => (deadline.saturating_duration_since(now), total),
            Self::Paused { remaining, total } => (remaining, total),
        };

        if total.is_zero() {
            return 0.0;
        }

        (remaining.as_secs_f32() / total.as_secs_f32()).clamp(0.0, 1.0)
    }
}

/// How far apart the steps are when animations are disabled. Short enough to
/// read as a countdown, long enough not to be an animation.
const STEP: Duration = Duration::from_millis(250);

/// A bar that shrinks as a countdown runs out.
///
/// It drives its own repaints from the deadline, the way the other animated
/// widgets here do, so nothing has to tick the application to keep it moving.
pub struct CountdownBar {
    countdown: Countdown,
    height: f32,
    color: Color,
    animated: bool,
}

pub fn countdown_bar(countdown: Countdown, height: f32, color: Color) -> CountdownBar {
    CountdownBar {
        countdown,
        height,
        color,
        animated: true,
    }
}

impl CountdownBar {
    /// When off, the bar steps every [`STEP`] instead of every frame.
    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer> for CountdownBar
where
    Renderer: iced::core::Renderer,
{
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(self.height))
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(limits, Length::Fill, Length::Fixed(self.height))
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &event::Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let event::Event::Window(window::Event::RedrawRequested(now)) = event else {
            return;
        };

        // A paused bar holds its width, and an empty one has nothing left to
        // show: neither needs waking up again.
        if !matches!(self.countdown, Countdown::Running { .. })
            || self.countdown.fraction(*now) <= 0.0
        {
            return;
        }

        if self.animated {
            shell.request_redraw();
        } else {
            shell.request_redraw_at(*now + STEP);
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        if viewport.intersection(&bounds).is_none() {
            return;
        }

        let fraction = self.countdown.fraction(Instant::now());
        let width = bounds.width * fraction;
        if width <= 0.0 {
            return;
        }

        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle { width, ..bounds },
                border: Border::default().rounded(bounds.height / 2.0),
                ..Default::default()
            },
            Background::Color(self.color),
        );
    }
}

impl<'a, Message, Theme, Renderer> From<CountdownBar> for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced::core::Renderer + 'a,
{
    fn from(bar: CountdownBar) -> Self {
        Self::new(bar)
    }
}
