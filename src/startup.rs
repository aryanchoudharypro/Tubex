use std::{
	fs::{self, File},
	io::{self, BufReader, Read, Write},
	path::{Path, PathBuf},
	process::Command,
	sync::{Arc, Mutex, mpsc::Sender},
	thread,
};

use crate::{config::ConfigManager, events::AppEvent};

pub fn check_dependencies(cfg: &Arc<Mutex<ConfigManager>>) -> (bool, bool) {
	let (yt, ff) = {
		let c = cfg.lock().expect("Config manager lock failed");
		(c.get_yt_dlp_path(), c.get_ffmpeg_path())
	};
	let check_tool = |path: &str, default: &str, arg: &str| -> bool {
		let try_run = |p: &str| Command::new(p).arg(arg).output().is_ok_and(|o| o.status.success());
		let p = if path.trim().is_empty() { default } else { path };
		try_run(p)
			|| (cfg!(windows) && !p.to_lowercase().ends_with(".exe") && try_run(&format!("{}.exe", p)))
			|| (p != default && (try_run(default) || (cfg!(windows) && try_run(&format!("{}.exe", default)))))
	};
	(check_tool(&yt, "yt-dlp", "--version"), check_tool(&ff, "ffmpeg", "-version"))
}

pub fn update_ytdlp(cfg: &Arc<Mutex<ConfigManager>>) {
	let (path, channel) = {
		let c = cfg.lock().expect("Config manager lock failed");
		(c.get_yt_dlp_path(), c.get_update_channel())
	};
	if Path::new(&path)
		.parent()
		.is_some_and(|p| p.as_os_str().is_empty() || p == std::env::current_dir().unwrap_or_default())
	{
		thread::spawn(move || {
			let mut cmd = Command::new(&path);
			cmd.arg("-U");
			if !channel.is_empty() && channel != "stable" {
				cmd.arg("--update-to").arg(&channel);
			}
			let _ = cmd.output();
		});
	}
}

pub fn download_tools_script(
	target_dir: &Path,
	tx: Sender<AppEvent>,
	missing_yt: bool,
	missing_ff: bool,
) -> Result<(), String> {
	let result: Result<(), String> = try {
		if !target_dir.exists() {
			fs::create_dir_all(target_dir).map_err(|e| e.to_string())?;
		}

		let download_file = |url: &str, filename: &str, start_pct: i32, end_pct: i32| -> Result<PathBuf, String> {
			let dest_path = target_dir.join(filename);
			let _ = tx.send(AppEvent::DownloadProgress(format!("Downloading {}...", filename), start_pct));
			let resp = ureq::get(url).call().map_err(|e| format!("Request failed: {}", e))?;
			let total_size = resp
				.headers()
				.get("content-length")
				.and_then(|h| h.to_str().ok())
				.and_then(|s| s.parse::<u64>().ok())
				.unwrap_or(0);
			let mut reader = resp.into_body().into_reader();
			let mut file = File::create(&dest_path).map_err(|e| format!("Failed to create file: {}", e))?;
			let mut buffer = [0; 8192];
			let mut downloaded: u64 = 0;
			loop {
				let n = reader.read(&mut buffer).map_err(|e| format!("Read error: {}", e))?;
				if n == 0 {
					break;
				}
				file.write_all(&buffer[..n]).map_err(|e| format!("Write error: {}", e))?;
				downloaded += n as u64;
				if total_size > 0 {
					let pct =
						start_pct + ((downloaded as f64 / total_size as f64) * (end_pct - start_pct) as f64) as i32;
					let _ = tx.send(AppEvent::DownloadProgress(format!("Downloading {}... ({}%)", filename, pct), pct));
				}
			}
			Ok(dest_path)
		};

		if missing_yt {
			download_file("https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe", "yt-dlp.exe", 0, 30)?;
		}

		if missing_ff {
			let zip_path = download_file(
				"https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
				"ffmpeg.zip",
				30,
				90,
			)?;
			let _ = tx.send(AppEvent::DownloadProgress("Extracting ffmpeg...".into(), 90));
			let file = File::open(&zip_path).map_err(|e| format!("Failed to open zip: {}", e))?;
			let mut archive = zip::ZipArchive::new(BufReader::new(file)).map_err(|e| format!("Zip error: {}", e))?;
			let mut found_any = false;
			for i in 0..archive.len() {
				let mut file = archive.by_index(i).map_err(|e| format!("Zip file error: {}", e))?;
				if let Some(path) = file.enclosed_name().map(|p| p.to_path_buf())
					&& let Some(name) = path.file_name().and_then(|n| n.to_str())
					&& name.to_lowercase().starts_with("ff")
					&& name.to_lowercase().ends_with(".exe")
				{
					let dest_path = target_dir.join(name);
					let mut outfile =
						File::create(&dest_path).map_err(|e| format!("Failed to create {}: {}", name, e))?;
					io::copy(&mut file, &mut outfile).map_err(|e| format!("Extraction failed for {}: {}", name, e))?;
					found_any = true;
				}
			}
			let _ = fs::remove_file(zip_path);
			if !found_any {
				return Err("No ffmpeg tools found in downloaded archive".into());
			}
		}
	};

	if result.is_ok() {
		let _ = tx.send(AppEvent::DownloadProgress("Done.".into(), 100));
	}
	result
}
