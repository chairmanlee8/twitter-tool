#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BoundingBox {
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
}

impl BoundingBox {
    pub fn new(left: u16, top: u16, width: u16, height: u16) -> Self {
        Self {
            left,
            top,
            width,
            height,
        }
    }
}
