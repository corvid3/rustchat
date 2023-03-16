mod broker;
use broker::{Broker, BrokerHandle};
use eframe::{egui::CentralPanel, App, CreationContext, NativeOptions};

struct Application {
  handle: BrokerHandle,
  history: String,
  current_input: String,
}

impl App for Application {
  fn update(&mut self, ctx: &eframe::egui::Context, frame: &mut eframe::Frame) {
    // check for any updates real quick
    while let Ok(msg) = self.handle.from_broker.try_recv() {
      self.history += &msg;
      self.history.push('\n');
    }

    // ok now draw
    CentralPanel::default().show(ctx, |ui| {
      ui.text_edit_multiline(&mut self.history);
      let input = ui.text_edit_singleline(&mut self.current_input);

      if input.lost_focus() && ctx.input(|i| i.key_pressed(eframe::egui::Key::Enter)) {
        self
          .handle
          .to_broker
          .blocking_send(self.current_input.clone())
          .unwrap();
        input.request_focus();
        self.current_input.clear();
      }
    });
  }
}

impl Application {
  fn new(cc: &CreationContext, handle: BrokerHandle) -> Self {
    Self {
      handle,
      history: String::new(),
      current_input: String::new(),
    }
  }
}

fn main() {
  let handle = Broker::new();

  eframe::run_native(
    "yacs2",
    NativeOptions::default(),
    Box::new(|cc| Box::new(Application::new(cc, handle))),
  )
  .unwrap();
}
