use crate::ui_framework::bounding_box::BoundingBox;
use anyhow::Result;
use crossterm::event::KeyEvent;
use std::io::Stdout;

pub mod bounding_box;
pub mod line_buffer;

pub trait Render {
    /// NB: [render] takes [&mut self] since there isn't a separate notification to component that
    /// their bbox changed.
    fn render(&mut self, stdout: &mut Stdout, bounding_box: BoundingBox) -> Result<()>;

    fn get_cursor(&self) -> (u16, u16);
}

pub trait Input {
    fn handle_focus(&mut self);
    fn handle_key_event(&mut self, event: &KeyEvent);
}

pub struct Component<T: Render + Input> {
    pub should_render: bool,
    pub bounding_box: BoundingBox,
    pub component: T,
}

impl<T: Render + Input> Component<T> {
    pub fn new(component: T) -> Self {
        Self {
            should_render: true,
            bounding_box: BoundingBox::default(),
            component,
        }
    }

    pub fn render_if_necessary(&mut self, stdout: &mut Stdout) -> Result<()> {
        if self.should_render {
            self.component.render(stdout, self.bounding_box)?;
            self.should_render = false;
        }
        Ok(())
    }

    pub fn get_cursor(&self) -> (u16, u16) {
        self.component.get_cursor()
    }
}
