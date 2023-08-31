
use iced::executor;
use iced::{Application, Command, Element, Settings, Theme};

pub struct App {
}

impl Application for App {
    type Executor = executor::Default;
    type Flags = ();
    type Message = ();
    type Theme = Theme;

    fn new(flags: Self::Flags) -> (Self, Command<Self::Message>) {
        todo!();
    }

    fn title(&self) -> String {
        String::from("A program to compose and edit timelines.")
    }

    fn update(&mut self, message: Self::Message) -> Command<Self::Message> {
        todo!();
    }

    fn view(&self) -> Element<'_, Self::Message> {
        todo!();
    }

}
