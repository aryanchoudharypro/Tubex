use std::{
	env, fs,
	path::{Path, PathBuf},
};

use configparser::ini::Ini;

const CONFIG_DIRECTORY: &str = "Tubex";
const CONFIG_FILENAME: &str = "Tubex.ini";
const CUSTOM_COMMANDS_SECTION: &str = "CustomCommands";
const SETTINGS_SECTION: &str = "Settings";

#[derive(Clone, Debug, PartialEq)]
pub struct CustomCommand {
	pub name: String,
	pub value: String,
}

pub struct ConfigManager {
	data: Ini,
	config_path: PathBuf,
}

impl ConfigManager {
	pub fn new() -> Self {
		let path = get_config_path();
		let mut data = Ini::new();
		if path.exists() {
			let _ = fs::read_to_string(&path).map(|c| data.read(c));
		}
		Self { data, config_path: path }
	}

	pub fn flush(&self) {
		self.config_path.parent().map(fs::create_dir_all);
		let _ = self.data.write(&self.config_path);
	}

	pub fn get_commands(&self) -> Vec<CustomCommand> {
		self.data
			.get_map_ref()
			.get("customcommands")
			.or_else(|| self.data.get_map_ref().get(CUSTOM_COMMANDS_SECTION))
			.map(|s| {
				s.iter()
					.filter_map(|(k, v)| v.as_ref().map(|val| CustomCommand { name: k.clone(), value: val.clone() }))
					.collect()
			})
			.unwrap_or_default()
	}

	pub fn set_commands(&mut self, commands: &[CustomCommand]) {
		self.data.remove_section(CUSTOM_COMMANDS_SECTION);
		for cmd in commands {
			self.data.set(CUSTOM_COMMANDS_SECTION, &cmd.name, Some(cmd.value.clone()));
		}
	}

	pub fn get_download_path(&self) -> Option<String> { self.data.get(SETTINGS_SECTION, "download_path") }

	pub fn set_download_path(&mut self, path: &str) {
		self.data.set(SETTINGS_SECTION, "download_path", Some(path.to_string()));
	}

	pub fn get_yt_dlp_path(&self) -> String {
		self.data.get(SETTINGS_SECTION, "yt_dlp_path").unwrap_or_else(|| "yt-dlp".to_string())
	}

	pub fn set_yt_dlp_path(&mut self, path: &str) {
		self.data.set(SETTINGS_SECTION, "yt_dlp_path", Some(path.to_string()));
	}

	pub fn get_ffmpeg_path(&self) -> String {
		self.data.get(SETTINGS_SECTION, "ffmpeg_path").unwrap_or_else(|| "ffmpeg".to_string())
	}

	pub fn set_ffmpeg_path(&mut self, path: &str) {
		self.data.set(SETTINGS_SECTION, "ffmpeg_path", Some(path.to_string()));
	}

	pub fn get_global_flags(&self) -> String { self.data.get(SETTINGS_SECTION, "global_flags").unwrap_or_default() }

	pub fn set_global_flags(&mut self, flags: &str) {
		self.data.set(SETTINGS_SECTION, "global_flags", Some(flags.to_string()));
	}

	pub fn get_update_channel(&self) -> String {
		self.data.get(SETTINGS_SECTION, "update_channel").unwrap_or_else(|| "stable".to_string())
	}

	pub fn set_update_channel(&mut self, channel: &str) {
		self.data.set(SETTINGS_SECTION, "update_channel", Some(channel.to_string()));
	}
}

fn get_config_path() -> PathBuf {
	let exe_path = env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
	let exe_dir = exe_path.parent().unwrap_or(Path::new("."));

	if is_directory_writable(exe_dir) {
		exe_dir.join(CONFIG_FILENAME)
	} else {
		config_root_dir().unwrap_or_else(|| exe_dir.to_path_buf()).join(CONFIG_DIRECTORY).join(CONFIG_FILENAME)
	}
}

fn is_directory_writable(path: &Path) -> bool {
	path.is_dir() && {
		let test_file = path.join(".tubex_write_test");
		fs::File::create(&test_file).map(|_| fs::remove_file(&test_file).is_ok()).unwrap_or(false)
	}
}

fn config_root_dir() -> Option<PathBuf> {
	#[cfg(windows)]
	{
		env::var("APPDATA").ok().map(PathBuf::from)
	}
	#[cfg(not(windows))]
	{
		env::var("XDG_CONFIG_HOME")
			.ok()
			.map(PathBuf::from)
			.or_else(|| env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config")))
	}
}
