//! Bounded, retractable presentation. Durable claims remain in the shared engine.
#[cfg(target_os = "linux")]
mod linux;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Category {
    Results,
    Pending,
}

#[cfg(target_os = "linux")]
impl Category {
    fn index(self) -> usize {
        match self {
            Self::Results => 0,
            Self::Pending => 1,
        }
    }
    fn title(self) -> &'static str {
        match self {
            Self::Results => "Fleetpane · Turn results",
            Self::Pending => "Fleetpane · Requests need attention",
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct Popup {
    generation: uuid::Uuid,
    category: Category,
    message: String,
}

#[cfg(target_os = "linux")]
type Desired = [Option<Popup>; 2];

#[cfg(target_os = "linux")]
#[derive(Clone, Debug, Default)]
struct Status {
    error: Option<String>,
    activation: Option<(u64, Category, uuid::Uuid)>,
}

#[derive(Default)]
pub(super) struct Controller {
    #[cfg(target_os = "linux")]
    desired: Desired,
    closing: bool,
    #[cfg(target_os = "linux")]
    worker: Option<linux::Worker>,
    #[cfg(target_os = "linux")]
    last_activation: u64,
}

impl Controller {
    pub fn show(&mut self, category: Category, message: String) {
        if self.closing {
            return;
        }
        #[cfg(target_os = "linux")]
        {
            self.desired[category.index()] = Some(Popup {
                generation: uuid::Uuid::new_v4(),
                category,
                message,
            });
            self.publish();
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (category, message);
        }
    }

    pub fn clear(&mut self, category: Category) {
        #[cfg(target_os = "linux")]
        if self.desired[category.index()].take().is_some() {
            self.publish();
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = category;
        }
    }

    #[cfg(target_os = "linux")]
    fn publish(&mut self) {
        #[cfg(target_os = "linux")]
        {
            let worker = self.worker.get_or_insert_with(linux::Worker::start);
            if let Some(commands) = &worker.commands {
                commands.send_replace(self.desired.clone());
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub fn reset(&mut self) {
        // Dropping the sender asks the one worker to close its owned notifications.
        // Consumed claims are deliberately not replayed after reconnect.
        self.desired = Default::default();
        self.closing = false;
        #[cfg(target_os = "linux")]
        {
            self.worker = None;
            self.last_activation = 0;
        }
    }

    pub fn close_ready(&mut self) -> bool {
        self.closing = true;
        #[cfg(target_os = "linux")]
        {
            self.desired = Default::default();
        }
        #[cfg(target_os = "linux")]
        if let Some(worker) = &mut self.worker {
            worker.commands.take();
            return worker.finished();
        }
        true
    }

    pub fn poll(&mut self) -> (Option<String>, Option<Category>) {
        #[cfg(target_os = "linux")]
        if let Some(worker) = &self.worker {
            let status = worker.status.borrow().clone();
            let mut action = None;
            if let Some((serial, category, generation)) = status.activation
                && serial != self.last_activation
            {
                self.last_activation = serial;
                if self.desired[category.index()]
                    .as_ref()
                    .is_some_and(|popup| popup.generation == generation)
                {
                    action = Some(category);
                }
            }
            return (status.error, action);
        }
        (None, None)
    }
}
