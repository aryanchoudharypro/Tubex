use std::{
	collections::HashMap,
	process::Child,
	sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct DownloadManager {
	history: Arc<Mutex<HashMap<String, String>>>,
	active_tasks: Arc<Mutex<HashMap<String, Arc<Mutex<Child>>>>>,
}

impl DownloadManager {
	pub fn new() -> Self {
		Self { history: Arc::new(Mutex::new(HashMap::new())), active_tasks: Arc::new(Mutex::new(HashMap::new())) }
	}

	pub fn register_task(&self, tag: String, child: Child) -> Arc<Mutex<Child>> {
		let shared = Arc::new(Mutex::new(child));
		self.active_tasks.lock().expect("Active tasks lock failed").insert(tag, shared.clone());
		shared
	}

	pub fn unregister_task(&self, tag: &str) {
		self.active_tasks.lock().expect("Active tasks lock failed").remove(tag);
	}

	pub fn cancel_task(&self, tag: &str) {
		if let Some(child) = self.active_tasks.lock().expect("Active tasks lock failed").get(tag) {
			let _ = child.lock().expect("Child lock failed").kill();
		}
	}

	pub fn append_output(&self, tag: &str, output: &str) {
		let mut history = self.history.lock().expect("History lock failed");
		history.entry(tag.to_string()).or_default().push_str(output);
	}

	pub fn get_output(&self, tag: &str) -> String {
		self.history.lock().expect("History lock failed").get(tag).cloned().unwrap_or_default()
	}

	pub fn has_task(&self, tag: &str) -> bool {
		self.active_tasks.lock().expect("Active tasks lock failed").contains_key(tag)
	}
}
