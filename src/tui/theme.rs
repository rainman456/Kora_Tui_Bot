use ratatui::style::Color;

pub struct Theme {
    pub primary: Color,
    pub secondary: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,
    pub background: Color,
    pub foreground: Color,
    pub border: Color,
}

impl Theme {
    pub fn default() -> Self {
        Self {
            primary: Color::Cyan,
            secondary: Color::Blue,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            info: Color::LightBlue,
            background: Color::Black,
            foreground: Color::White,
            border: Color::Gray,
        }
    }
    
    pub fn dark() -> Self {
        Self {
            primary: Color::Rgb(100, 200, 255),
            secondary: Color::Rgb(150, 150, 255),
            success: Color::Rgb(100, 255, 100),
            warning: Color::Rgb(255, 200, 100),
            error: Color::Rgb(255, 100, 100),
            info: Color::Rgb(150, 200, 255),
            background: Color::Rgb(20, 20, 30),
            foreground: Color::Rgb(220, 220, 230),
            border: Color::Rgb(60, 60, 80),
        }
    }
}