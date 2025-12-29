#![windows_subsystem = "windows"]
#![feature(try_blocks)]

mod config;
mod config_dialog;
mod download_manager;
mod events;
mod options_dialog;
mod search_tab;
mod settings_tab;
mod startup;
mod video_info;

use std::{
	io::{BufRead, BufReader},
	os::windows::process::CommandExt,
	process::{Command, Stdio},
	sync::{Arc, Mutex, mpsc},
	thread,
};

use config_dialog::show_config_dialog;
use download_manager::DownloadManager;
use events::AppEvent;
use options_dialog::{DownloadMode, DownloadOptions, show_options_dialog, show_playlist_dialog};
use search_tab::create_search_tab;
use settings_tab::create_settings_tab;
use video_info::VideoInfo;
use wxdragon::{
	PanelStyle, TextCtrlStyle, clipboard,
	prelude::*,
	widgets::{Choice, Gauge, ListBox, Notebook},
};

const CREATE_NO_WINDOW: u32 = 0x08000000;

fn main() {
	let _ = wxdragon::main(|_| {
		let config_manager = Arc::new(Mutex::new(config::ConfigManager::new()));
		let download_manager = Arc::new(DownloadManager::new());

		let frame = Frame::builder().with_title("Tubex Downloader").with_size(Size::new(900, 750)).build();
		let notebook = Notebook::builder(&frame).build();

		let downloader_panel = Panel::builder(&notebook).with_style(PanelStyle::TabTraversal).build();
		notebook.add_page(&downloader_panel, "Downloader", true, None);

		let sub_notebook = Notebook::builder(&downloader_panel).build();

		let input_panel = Panel::builder(&sub_notebook).with_style(PanelStyle::TabTraversal).build();
		let input_sizer = BoxSizer::builder(Orientation::Vertical).build();

		let url_label = StaticText::builder(&input_panel).with_label("Video URLs (One per line):").build();
		let url_text_ctrl = TextCtrl::builder(&input_panel).with_value("").with_style(TextCtrlStyle::MultiLine).build();

		input_sizer.add(&url_label, 0, SizerFlag::All, 5);
		input_sizer.add(&url_text_ctrl, 1, SizerFlag::Expand | SizerFlag::All, 5);
		input_panel.set_sizer(input_sizer, true);

		sub_notebook.add_page(&input_panel, "Add URLs", true, None);

		let (tx, rx) = mpsc::channel::<AppEvent>();

		let (search_panel, search_ctx) = create_search_tab(&sub_notebook, config_manager.clone(), tx.clone());
		sub_notebook.add_page(&search_panel, "Search", false, None);

		let current_tab = Arc::new(Mutex::new(0));
		let ct_clone = current_tab.clone();
		sub_notebook.on_page_changed(move |e| {
			if let Some(sel) = e.get_selection() {
				*ct_clone.lock().expect("Current tab lock failed") = sel as usize;
			}
		});

		let settings_panel = create_settings_tab(&notebook, config_manager.clone());
		notebook.add_page(&settings_panel, "Settings", false, None);

		let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
		main_sizer.add(&sub_notebook, 1, SizerFlag::Expand | SizerFlag::All, 5);

		let commands_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let commands_label = StaticText::builder(&downloader_panel).with_label("Custom Command:").build();
		let get_current_choices = |cfg_m: &Arc<Mutex<config::ConfigManager>>| {
			cfg_m
				.lock()
				.map(|c| {
					std::iter::once("No Command".to_string())
						.chain(c.get_commands().iter().map(|cmd| cmd.name.clone()))
						.collect()
				})
				.unwrap_or_else(|_| vec!["No Command".to_string()])
		};

		let choices = get_current_choices(&config_manager);
		let commands_choice = Choice::builder(&downloader_panel).with_choices(choices).build();
		commands_choice.set_selection(0);

		let configure_button = Button::builder(&downloader_panel).with_label("Configure...").build();
		commands_sizer.add(&commands_label, 0, SizerFlag::AlignCenterVertical | SizerFlag::All, 5);
		commands_sizer.add(&commands_choice, 1, SizerFlag::Expand | SizerFlag::All, 5);
		commands_sizer.add(&configure_button, 0, SizerFlag::All, 5);
		main_sizer.add_sizer(&commands_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

		let download_button = Button::builder(&downloader_panel).with_label("Download").build();
		main_sizer.add(&download_button, 0, SizerFlag::AlignCenterHorizontal | SizerFlag::All, 10);

		let list_label =
			StaticText::builder(&downloader_panel).with_label("Downloads Status (Select to view output)").build();
		main_sizer.add(&list_label, 0, SizerFlag::All, 5);

		let status_list = ListBox::builder(&downloader_panel).build();
		main_sizer.add(&status_list, 1, SizerFlag::Expand | SizerFlag::All, 5);

		let cancel_button = Button::builder(&downloader_panel).with_label("Cancel Selected Download").build();
		cancel_button.enable(false);
		main_sizer.add(&cancel_button, 0, SizerFlag::All | SizerFlag::AlignRight, 5);

		let log_label = StaticText::builder(&downloader_panel).with_label("Command Output").build();
		main_sizer.add(&log_label, 0, SizerFlag::All, 5);

		let output_text =
			TextCtrl::builder(&downloader_panel).with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly).build();
		main_sizer.add(&output_text, 2, SizerFlag::Expand | SizerFlag::All, 5);

		downloader_panel.set_sizer(main_sizer, true);

		let timer = Box::leak(Box::new(Timer::new(&frame)));
		let list_clone_timer = status_list;
		let output_clone_timer = output_text;
		let frame_clone_timer = frame;
		let tx_clone_timer = tx.clone();
		let dm_timer = Arc::clone(&download_manager);
		let cfg_timer = Arc::clone(&config_manager);
		let cancel_btn_timer = cancel_button;

		let download_list_ids = Arc::new(Mutex::new(Vec::<String>::new()));
		let ids_timer = Arc::clone(&download_list_ids);
		let ids_sel = Arc::clone(&download_list_ids);
		let ids_cancel = Arc::clone(&download_list_ids);
		let ids_get_tag = Arc::clone(&download_list_ids);

		let list_tag_lookup = list_clone_timer;
		let get_selected_tag = move || {
			list_tag_lookup
				.get_selection()
				.and_then(|idx| ids_get_tag.lock().ok().and_then(|ids| ids.get(idx as usize).cloned()))
		};

		let download_dialog = std::rc::Rc::new(std::cell::RefCell::new(None));
		let dd_clone = download_dialog.clone();

		let url_clone_timer = url_text_ctrl;
		let last_clipboard = std::rc::Rc::new(std::cell::RefCell::new(String::new()));

		let _ = tx.send(AppEvent::StartupCheck);
		timer.start(100, false);

		timer.on_tick(move |_| {
			let update_item = |tag: &str, new_text: &str| {
				let mut ids = ids_timer.lock().expect("IDs lock failed");
				if let Some(pos) = ids.iter().position(|t| t == tag) {
					ids.remove(pos);
					list_clone_timer.delete(pos as u32);
				}
				ids.push(tag.to_string());
				list_clone_timer.append(new_text);
			};

			if let Some(text) = clipboard::Clipboard::get().get_text() {
				let trimmed = text.trim().to_string();
				let mut last = last_clipboard.borrow_mut();
				if *last != trimmed {
					if trimmed.contains("youtube.com") || trimmed.contains("youtu.be") {
						let current = url_clone_timer.get_value();
						if !current.contains(&trimmed) {
							if !current.is_empty() && !current.ends_with('\n') {
								url_clone_timer.append_text("\n");
							}
							url_clone_timer.append_text(&trimmed);
						}
					}
					*last = trimmed;
				}
			}

			let mut pending_log_update = String::new();
			let mut loops = 0;

			while let Ok(event) = rx.try_recv() {
				loops += 1;
				if loops > 2000 {
					break;
				}

				match event {
					AppEvent::RequestFetch(url) => {
						fetch_info(url, tx_clone_timer.clone(), Arc::clone(&cfg_timer), Arc::clone(&dm_timer));
					}
					AppEvent::StartupCheck => {
						let c = Arc::clone(&cfg_timer);
						let t = tx_clone_timer.clone();
						thread::spawn(move || {
							let (yt, ff) = startup::check_dependencies(&c);
							let _ = t.send(AppEvent::StartupResult(yt, ff));
							startup::update_ytdlp(&c);
						});
					}
					AppEvent::StartupResult(yt_ok, ffmpeg_ok) => {
						if (!yt_ok || !ffmpeg_ok) && show_setup_dialog(&frame_clone_timer, (yt_ok, ffmpeg_ok)) == 2 {
							*dd_clone.borrow_mut() = Some(show_download_progress(&frame_clone_timer));
							let tx_dl = tx_clone_timer.clone();
							thread::spawn(move || {
								let exe_dir = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
								let target_dir = exe_dir.parent().unwrap_or(std::path::Path::new("."));
								let _ = tx_dl.send(AppEvent::DownloadComplete(startup::download_tools_script(
									target_dir,
									tx_dl.clone(),
									!yt_ok,
									!ffmpeg_ok,
								)));
							});
						}
					}
					AppEvent::DownloadComplete(res) => {
						if let Some((d, _)) = dd_clone.borrow_mut().take() {
							d.close(true);
						}
						match res {
							Ok(_) => {
								if let Ok(mut c) = cfg_timer.lock() {
									let exe_dir =
										std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("."));
									let target_dir = exe_dir.parent().unwrap_or(std::path::Path::new("."));
									c.set_yt_dlp_path(&target_dir.join("yt-dlp.exe").to_string_lossy());
									c.set_ffmpeg_path(&target_dir.join("ffmpeg.exe").to_string_lossy());
									c.set_update_channel("stable");
									c.flush();
								}
								let _ = MessageDialog::builder(
									&frame_clone_timer,
									"Download complete! Tools are ready.",
									"Success",
								)
								.build()
								.show_modal();
							}
							Err(e) => {
								let _ = MessageDialog::builder(
									&frame_clone_timer,
									&format!("Download failed: {}", e),
									"Error",
								)
								.build()
								.show_modal();
							}
						}
					}
					AppEvent::Status(tag, msg) => {
						update_item(&tag, &msg);
						if list_clone_timer.get_count() > 0 {
							list_clone_timer.set_selection(list_clone_timer.get_count() - 1, true);
							output_clone_timer.set_value(&dm_timer.get_output(&tag));
							cancel_btn_timer.enable(true);
						}
					}
					AppEvent::Output(tag, msg) => {
						dm_timer.append_output(&tag, &msg);
						if get_selected_tag().is_some_and(|s| s == tag) {
							pending_log_update.push_str(&msg);
							pending_log_update.push('\n');
						}
					}
					AppEvent::Finished(url) => {
						update_item(&url, &format!("Finished: {}", url));
						if get_selected_tag().is_some_and(|s| s == url) {
							cancel_btn_timer.enable(false);
							output_clone_timer.append_text("--- Finished ---\n");
						}
						dm_timer.unregister_task(&url);
					}
					AppEvent::Error(tag, err_msg) => {
						update_item(&tag, &format!("Error: {}", tag));
						dm_timer.append_output(&tag, &format!("Error: {}", err_msg));
						if get_selected_tag().is_some_and(|s| s == tag) {
							cancel_btn_timer.enable(false);
							output_clone_timer.append_text(&format!("Error: {}\n", err_msg));
						}
						dm_timer.unregister_task(&tag);
					}
					AppEvent::DownloadProgress(msg, val) => {
						if let Some((d, g)) = &*dd_clone.borrow() {
							d.set_label(&msg);
							g.set_value(val);
						}
					}
					AppEvent::ShowOptions(url, infos) => {
						if let Some(opts) = if infos.len() > 1 {
							show_playlist_dialog(&frame_clone_timer, &infos)
						} else {
							infos.first().and_then(|i| show_options_dialog(&frame_clone_timer, i))
						} {
							start_batch_download(
								vec![url],
								None,
								Some(opts),
								tx_clone_timer.clone(),
								Arc::clone(&dm_timer),
								Arc::clone(&cfg_timer),
							);
						}
					}
					AppEvent::ShowOptionsForMultipleUrls(urls, first_video_info) => {
						if let Some(opts) = show_options_dialog(&frame_clone_timer, &first_video_info) {
							start_batch_download(
								urls,
								None,
								Some(opts.clone()),
								tx_clone_timer.clone(),
								Arc::clone(&dm_timer),
								Arc::clone(&cfg_timer),
							);
						}
					}
				}
			}

			if !pending_log_update.is_empty() {
				if output_clone_timer.get_last_position() > 60_000 {
					if let Some(tag) = get_selected_tag() {
						output_clone_timer.set_value(&dm_timer.get_output(&tag));
					}
				} else {
					output_clone_timer.append_text(&pending_log_update);
				}
			}
		});

		let cfg_for_cfg = Arc::clone(&config_manager);
		let frame_for_dialog = frame;
		let choice_clone = commands_choice;
		configure_button.on_click(move |_| {
			show_config_dialog(&frame_for_dialog, Arc::clone(&cfg_for_cfg));
			choice_clone.clear();
			for c in get_current_choices(&cfg_for_cfg) {
				choice_clone.append(&c);
			}
			choice_clone.set_selection(0);
		});

		let status_list_sel = status_list;
		let output_text_sel = output_text;
		let dm_sel = Arc::clone(&download_manager);
		let cancel_btn_sel = cancel_button;
		status_list.on_selection_changed(move |event| {
			if let Some(idx) = event.get_selection() {
				if let Some(ids) = ids_sel.lock().ok()
					&& let Some(tag) = ids.get(idx as usize)
					&& let Some(s) = status_list_sel.get_string(idx as u32)
				{
					cancel_btn_sel.enable(s.starts_with("Started:") || s.starts_with("Fetching info:"));
					output_text_sel.set_value(&dm_sel.get_output(tag));
				}
			} else {
				cancel_btn_sel.enable(false);
			}
		});

		let status_list_cancel = status_list;
		let dm_cancel = Arc::clone(&download_manager);
		cancel_button.on_click(move |_| {
			if let Some(idx) = status_list_cancel.get_selection()
				&& let Some(ids) = ids_cancel.lock().ok()
				&& let Some(tag) = ids.get(idx as usize)
				&& dm_cancel.has_task(tag)
			{
				dm_cancel.cancel_task(tag);
			}
		});

		let url_tc = url_text_ctrl;
		let cmd_choice_clone = commands_choice;
		let cfg_for_download = Arc::clone(&config_manager);
		let tx_for_download = tx.clone();
		let dm_download = Arc::clone(&download_manager);
		let search_sel = search_ctx.selected_videos.clone();
		let tab_tracker_btn = current_tab.clone();
		let sub_nb_download = sub_notebook;

		download_button.on_click(move |_| {
			let sel_idx = tab_tracker_btn.lock().map(|t| *t).unwrap_or(0);
			let sel = cmd_choice_clone.get_selection().unwrap_or(0);
			let raw_text = url_tc.get_value();

			handle_download_action(
				sel_idx,
				sel as i32,
				raw_text,
				search_sel.clone(),
				cfg_for_download.clone(),
				tx_for_download.clone(),
				dm_download.clone(),
				&sub_nb_download,
			);
		});
		let cfg_for_close = Arc::clone(&config_manager);
		frame.on_close(move |_| {
			if let Ok(c) = cfg_for_close.lock() {
				c.flush();
			}
		});

		frame.layout();
		frame.centre();
		frame.show(true);
	});
}

fn handle_download_action(
	sel_idx: usize,
	sel: i32,
	raw_text: String,
	search_sel: Arc<Mutex<Vec<VideoInfo>>>,
	cfg: Arc<Mutex<config::ConfigManager>>,
	tx: mpsc::Sender<AppEvent>,
	dm: Arc<DownloadManager>,
	parent: &Window,
) {
	let cmd_str = if sel > 0 {
		cfg.lock()
			.ok()
			.and_then(|cfg| cfg.get_commands().get((sel - 1) as usize).map(|c| c.value.clone()))
			.unwrap_or_default()
	} else {
		String::new()
	};

	if sel_idx == 0 {
		let urls: Vec<String> = raw_text.lines().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
		if urls.is_empty() {
			return;
		}

		if sel == 0 && urls.len() > 1 {
			let first_url_clone = urls[0].clone();
			let all_urls_clone = urls.clone();
			let tx_info_fetch = tx.clone();
			let cfg_info_fetch = cfg.clone();

			thread::spawn(move || {
				let (yt_dlp_path, global_flags) = match cfg_info_fetch.lock() {
					Ok(c) => (c.get_yt_dlp_path(), c.get_global_flags()),
					Err(_) => {
						let _ =
							tx_info_fetch.send(AppEvent::Error(first_url_clone.clone(), "Config lock failed".into()));
						return;
					}
				};
				let mut cmd = Command::new(&yt_dlp_path);
				let parsed_flags = shell_words::split(&global_flags)
					.unwrap_or_else(|_| global_flags.split_whitespace().map(String::from).collect());
				for arg in &parsed_flags {
					cmd.arg(arg);
				}
				cmd.arg("--dump-json").arg(&first_url_clone);
				cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(CREATE_NO_WINDOW);
				match cmd.spawn() {
					Ok(mut child) => {
						let stdout = child.stdout.take();
						let mut videos = Vec::new();
						if let Some(out) = stdout {
							for l in BufReader::new(out).lines().map_while(Result::ok) {
								if l.trim().is_empty() {
									continue;
								}
								if l.starts_with('{')
									&& let Ok(info) = serde_json::from_str::<VideoInfo>(&l)
								{
									videos.push(info);
								}
							}
						}
						if let Some(first_video_info) = videos.first().cloned() {
							let _ = tx_info_fetch
								.send(AppEvent::ShowOptionsForMultipleUrls(all_urls_clone, Box::new(first_video_info)));
						} else {
							let _ = tx_info_fetch
								.send(AppEvent::Error(first_url_clone, "No video info found for first URL.".into()));
						}
					}
					Err(e) => {
						let _ = tx_info_fetch.send(AppEvent::Error(first_url_clone, format!("Spawn failed: {}", e)));
					}
				}
			});
		} else {
			for url in urls {
				if sel == 0 {
					fetch_info(url, tx.clone(), cfg.clone(), Arc::clone(&dm));
				} else if !cmd_str.is_empty() {
					start_batch_download(
						vec![url],
						Some(cmd_str.clone()),
						None,
						tx.clone(),
						Arc::clone(&dm),
						cfg.clone(),
					);
				}
			}
		}
	} else if sel_idx == 1 {
		let selected_videos = search_sel.lock().expect("Search selection lock failed");
		if selected_videos.is_empty() {
			let _ = MessageDialog::builder(
				parent,
				"Please select one or more videos from the search results first.",
				"No Selection",
			)
			.build()
			.show_modal();
		} else if sel == 0 && selected_videos.len() > 1 {
			let urls: Vec<String> = selected_videos
				.iter()
				.map(|video| {
					video.webpage_url.clone().unwrap_or_else(|| {
						video.url.clone().unwrap_or_else(|| format!("https://www.youtube.com/watch?v={}", video.id))
					})
				})
				.collect();
			let first_url_clone = urls[0].clone();
			let all_urls_clone = urls.clone();
			let tx_info_fetch = tx.clone();
			let cfg_info_fetch = cfg.clone();

			thread::spawn(move || {
				let (yt_dlp_path, global_flags) = match cfg_info_fetch.lock() {
					Ok(c) => (c.get_yt_dlp_path(), c.get_global_flags()),
					Err(_) => {
						let _ =
							tx_info_fetch.send(AppEvent::Error(first_url_clone.clone(), "Config lock failed".into()));
						return;
					}
				};
				let mut cmd = Command::new(&yt_dlp_path);
				let parsed_flags = shell_words::split(&global_flags)
					.unwrap_or_else(|_| global_flags.split_whitespace().map(String::from).collect());
				for arg in &parsed_flags {
					cmd.arg(arg);
				}
				cmd.arg("--dump-json").arg(&first_url_clone);
				cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(CREATE_NO_WINDOW);
				match cmd.spawn() {
					Ok(mut child) => {
						let stdout = child.stdout.take();
						let mut videos = Vec::new();
						if let Some(out) = stdout {
							for l in BufReader::new(out).lines().map_while(Result::ok) {
								if l.trim().is_empty() {
									continue;
								}
								if l.starts_with('{')
									&& let Ok(info) = serde_json::from_str::<VideoInfo>(&l)
								{
									videos.push(info);
								}
							}
						}
						if let Some(first_video_info) = videos.first().cloned() {
							let _ = tx_info_fetch
								.send(AppEvent::ShowOptionsForMultipleUrls(all_urls_clone, Box::new(first_video_info)));
						} else {
							let _ = tx_info_fetch.send(AppEvent::Error(
								first_url_clone,
								"No video info found for first selected item.".into(),
							));
						}
					}
					Err(e) => {
						let _ = tx_info_fetch.send(AppEvent::Error(first_url_clone, format!("Spawn failed: {}", e)));
					}
				}
			});
		} else {
			for video in selected_videos.iter() {
				let url = video.webpage_url.clone().unwrap_or_else(|| {
					video.url.clone().unwrap_or_else(|| format!("https://www.youtube.com/watch/v={}", video.id))
				});
				if sel == 0 {
					fetch_info(url, tx.clone(), cfg.clone(), Arc::clone(&dm));
				} else if !cmd_str.is_empty() {
					start_batch_download(
						vec![url],
						Some(cmd_str.clone()),
						None,
						tx.clone(),
						Arc::clone(&dm),
						cfg.clone(),
					);
				}
			}
		}
	}
}

fn expand_env_vars(arg: &str) -> String {
	let mut expanded = arg.to_string();
	let mut start = 0;
	while let Some(pos) = expanded[start..].find('%') {
		let actual_pos = start + pos;
		if let Some(end_pos) = expanded[actual_pos + 1..].find('%') {
			let actual_end = actual_pos + 1 + end_pos;
			let var_name = &expanded[actual_pos + 1..actual_end];
			if let Ok(val) = std::env::var(var_name) {
				expanded.replace_range(actual_pos..actual_end + 1, &val);
				start = actual_pos + val.len();
			} else {
				start = actual_end + 1;
			}
		} else {
			break;
		}
	}
	if cfg!(windows) {
		expanded = expanded.replace("/", "\\");
	}
	expanded
}

fn fetch_info(
	url: String,
	tx: mpsc::Sender<AppEvent>,
	cfg: Arc<Mutex<config::ConfigManager>>,
	dm: Arc<DownloadManager>,
) {
	let (yt_dlp_path, global_flags) = match cfg.lock() {
		Ok(c) => (c.get_yt_dlp_path(), c.get_global_flags()),
		Err(_) => {
			let _ = tx.send(AppEvent::Error(url.clone(), "Configuration lock failed".into()));
			return;
		}
	};
	let tag = url.clone();
	let _ = tx.send(AppEvent::Status(tag.clone(), format!("Fetching info: {}", url)));
	let _ = tx.send(AppEvent::Output(
		tag.clone(),
		format!("Running: {} {} --dump-json --flat-playlist \"{}\"", yt_dlp_path, global_flags, url),
	));

	let tx_out = tx.clone();
	let tx_err = tx.clone();
	let tag_out = tag.clone();
	let tag_err = tag.clone();
	let dm_clone = dm.clone();
	let tag_cleanup = tag.clone();
	let yt_path = yt_dlp_path.clone();

	thread::spawn(move || {
		let mut cmd = Command::new(&yt_dlp_path);
		cmd.env("PYTHONIOENCODING", "utf-8");
		cmd.arg("--encoding").arg("utf-8");
		let parsed_flags = shell_words::split(&global_flags)
			.unwrap_or_else(|_| global_flags.split_whitespace().map(String::from).collect());
		for arg in &parsed_flags {
			cmd.arg(expand_env_vars(arg));
		}
		cmd.arg("--dump-json").arg("--flat-playlist").arg(&url);
		cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(CREATE_NO_WINDOW);

		match cmd.spawn() {
			Ok(mut child) => {
				let stdout = child.stdout.take();
				let stderr = child.stderr.take();
				let shared_child = dm.register_task(tag_out.clone(), child);

				if let Some(err) = stderr {
					thread::spawn(move || {
						for l in BufReader::new(err).lines().map_while(Result::ok) {
							let _ = tx_err.send(AppEvent::Output(tag_err.clone(), format!("[Info] {}", l)));
						}
					});
				}

				let mut videos = Vec::new();
				if let Some(out) = stdout {
					for l in BufReader::new(out).lines().map_while(Result::ok) {
						if l.trim().is_empty() {
							continue;
						}
						if l.starts_with('{') {
							match serde_json::from_str::<VideoInfo>(&l) {
								Ok(info) => videos.push(info),
								Err(e) => {
									let _ = tx_out
										.send(AppEvent::Output(tag_out.clone(), format!("JSON Parse Warning: {}", e)));
								}
							}
						} else {
							let _ = tx_out.send(AppEvent::Output(tag_out.clone(), l));
						}
					}
				}

				loop {
					thread::sleep(std::time::Duration::from_millis(100));
					let mut killed = false;
					let status = {
						let mut c = shared_child.lock().expect("Child lock failed");
						match c.try_wait() {
							Ok(Some(s)) => Some(s),
							Ok(None) => None,
							Err(_) => {
								killed = true;
								None
							}
						}
					};

					if killed {
						let _ = tx_out.send(AppEvent::Error(tag_out.clone(), "Cancelled".into()));
						break;
					}
					if let Some(s) = status {
						if s.success() {
							if !videos.is_empty() {
								if videos.first().is_some_and(|v| v.formats.is_empty()) {
									let _ = tx_out
										.send(AppEvent::Status(tag_out.clone(), "Fetching detailed formats...".into()));
									let mut cmd2 = Command::new(&yt_path);
									for arg in &parsed_flags {
										cmd2.arg(expand_env_vars(arg));
									}
									cmd2.arg("--dump-json")
										.arg("--playlist-items")
										.arg("1")
										.arg(&url)
										.creation_flags(CREATE_NO_WINDOW);
									if let Ok(output) = cmd2.output()
										&& output.status.success() && let Ok(full_json) =
										String::from_utf8(output.stdout)
										&& let Ok(ref_video) = serde_json::from_str::<VideoInfo>(&full_json)
										&& !videos.is_empty()
									{
										videos[0] = ref_video;
									}
								}
								let _ = tx_out.send(AppEvent::ShowOptions(tag_out.clone(), videos));
								let _ = tx_out.send(AppEvent::Finished(tag_out.clone()));
							} else {
								let _ = tx_out
									.send(AppEvent::Error(tag_out.clone(), "No valid video information found.".into()));
							}
						} else {
							let _ = tx_out.send(AppEvent::Error(
								tag_out.clone(),
								format!("Process exited with code {:?}", s.code()),
							));
						}
						break;
					}
					if !dm_clone.has_task(&tag_out) {
						break;
					}
				}
				dm_clone.unregister_task(&tag_cleanup);
			}
			Err(e) => {
				let _ = tx_out.send(AppEvent::Error(tag_out, format!("Spawn failed: {}", e)));
			}
		}
	});
}

fn start_batch_download(
	urls: Vec<String>,
	custom_cmd: Option<String>,
	opts: Option<DownloadOptions>,
	tx: mpsc::Sender<AppEvent>,
	dm: Arc<DownloadManager>,
	cfg: Arc<Mutex<config::ConfigManager>>,
) {
	if urls.is_empty() {
		return;
	}
	let tag = urls[0].clone();
	let status_msg = if urls.len() > 1 {
		format!("Started batch of {}: {}", urls.len(), &tag)
	} else {
		format!("Started: {}", &tag)
	};
	let _ = tx.send(AppEvent::Status(tag.clone(), status_msg));

	let (yt_dlp_path, download_path, ffmpeg_path, global_flags) = if let Ok(c) = cfg.lock() {
		(c.get_yt_dlp_path(), c.get_download_path(), c.get_ffmpeg_path(), c.get_global_flags())
	} else {
		let _ = tx.send(AppEvent::Error(tag, "Config lock failed".into()));
		return;
	};

	let urls_clone = urls.clone();
	thread::spawn(move || {
		let mut cmd = Command::new(&yt_dlp_path);
		cmd.env("PYTHONIOENCODING", "utf-8");
		cmd.arg("--encoding").arg("utf-8");
		let args_iter = shell_words::split(&global_flags)
			.unwrap_or_else(|_| global_flags.split_whitespace().map(String::from).collect());
		for arg in args_iter {
			cmd.arg(expand_env_vars(&arg));
		}

		if let Some(dp) = download_path
			&& !dp.is_empty()
		{
			cmd.current_dir(dp);
		}
		if !ffmpeg_path.is_empty() && ffmpeg_path != "ffmpeg" {
			let p = std::path::Path::new(&ffmpeg_path);
			if p.is_absolute() || p.parent().is_some_and(|parent| !parent.as_os_str().is_empty()) {
				cmd.arg("--ffmpeg-location").arg(ffmpeg_path);
			}
		}

		if let Some(o) = opts {
			let (v_format, a_formats) = match &o.mode {
				DownloadMode::Single { video_format, audio_formats } => (video_format.clone(), audio_formats.clone()),
				DownloadMode::Playlist { video_format, audio_formats, .. } => {
					(video_format.clone(), audio_formats.clone())
				}
			};
			let get_base_fmt = |f: &str| f.split('-').next().unwrap_or(f).to_string();
			let mut primary_f = String::new();
			let mut fuzzy_f = String::new();
			let mut lang_f = String::new();

			if let Some(v) = &v_format {
				primary_f.push_str(v);
				let v_base = get_base_fmt(v);
				lang_f.push_str(&if !a_formats.is_empty() {
					format!("bv[format_id^='{}']", v_base)
				} else {
					format!("b[format_id^='{}']", v_base)
				});
				fuzzy_f.push_str(if !a_formats.is_empty() { "bv" } else { "b" });
			}

			if !a_formats.is_empty() {
				if !primary_f.is_empty() {
					primary_f.push('+');
					fuzzy_f.push('+');
					lang_f.push('+');
				}
				primary_f.push_str(&a_formats.join("+"));
				fuzzy_f.push_str(
					&a_formats
						.iter()
						.map(|a| format!("ba[format_id^='{}']", get_base_fmt(a)))
						.collect::<Vec<_>>()
						.join("+"),
				);
				if o.preferred_languages.len() == a_formats.len() {
					lang_f.push_str(
						&o.preferred_languages
							.iter()
							.zip(a_formats.iter())
							.map(|(l, f)| format!("ba[language='{}'][format_id^='{}']", l, get_base_fmt(f)))
							.collect::<Vec<_>>()
							.join("+"),
					);
				} else {
					lang_f.clear();
				}
			} else {
				lang_f.clear();
			}

			if !primary_f.is_empty() {
				let mut f_selector = String::new();
				if !lang_f.is_empty() {
					f_selector.push_str(&lang_f);
					f_selector.push('/');
				}
				f_selector.push_str(&primary_f);
				f_selector.push('/');
				f_selector.push_str(&fuzzy_f);
				cmd.arg("-f").arg(&f_selector);
				let _ = tx.send(AppEvent::Output(tag.clone(), format!("Format Selector: {}", f_selector)));
			}

			if let DownloadMode::Playlist { indices, .. } = &o.mode {
				cmd.arg("--playlist-items").arg(indices.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","));
			}
			if o.add_chapters {
				cmd.arg("--embed-chapters");
			}
			if o.multi_audio || a_formats.len() > 1 {
				cmd.arg("--audio-multistreams");
			}
		}

		if let Some(c) = custom_cmd {
			let c_args = shell_words::split(&c).unwrap_or_else(|_| c.split_whitespace().map(String::from).collect());
			for arg in c_args {
				cmd.arg(expand_env_vars(&arg));
			}
		}

		for url in &urls_clone {
			cmd.arg(url);
		}

		let cmd_str = format!("{:?} {:?}", cmd.get_program(), cmd.get_args().collect::<Vec<_>>());
		let _ = tx.send(AppEvent::Output(tag.clone(), format!("Executing: {}", cmd_str)));

		cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(CREATE_NO_WINDOW);

		match cmd.spawn() {
			Ok(mut child) => {
				let stdout = child.stdout.take();
				let stderr = child.stderr.take();
				let shared_child = dm.register_task(tag.clone(), child);
				let tx_out = tx.clone();
				let tag_out = tag.clone();
				if let Some(out) = stdout {
					thread::spawn(move || {
						for l in BufReader::new(out).lines().map_while(Result::ok) {
							let _ = tx_out.send(AppEvent::Output(tag_out.clone(), l));
						}
					});
				}
				let tx_err = tx.clone();
				let tag_err = tag.clone();
				if let Some(err) = stderr {
					thread::spawn(move || {
						for l in BufReader::new(err).lines().map_while(Result::ok) {
							let _ = tx_err.send(AppEvent::Output(tag_err.clone(), format!("[Err] {}", l)));
						}
					});
				}

				loop {
					thread::sleep(std::time::Duration::from_millis(200));
					let mut killed = false;
					let status = {
						let mut c = shared_child.lock().expect("Child lock failed");
						match c.try_wait() {
							Ok(Some(s)) => Some(s),
							Ok(None) => None,
							Err(_) => {
								killed = true;
								None
							}
						}
					};
					if killed {
						let _ = tx.send(AppEvent::Error(tag.clone(), "Process killed".into()));
						break;
					}
					if let Some(s) = status {
						if s.success() {
							let _ = tx.send(AppEvent::Finished(tag.clone()));
						} else {
							let _ = tx.send(AppEvent::Error(tag.clone(), format!("Exit: {:?}", s)));
						}
						break;
					}
					if !dm.has_task(&tag) {
						break;
					}
				}
				dm.unregister_task(&tag);
			}
			Err(e) => {
				let _ = tx.send(AppEvent::Error(tag, format!("Spawn failed: {}", e)));
			}
		}
	});
}

fn show_setup_dialog(parent: &Window, (yt_ok, ff_ok): (bool, bool)) -> i32 {
	let dialog = Dialog::builder(parent, "Setup Required").with_size(400, 250).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();
	let mut missing = Vec::new();
	if !yt_ok {
		missing.push("yt-dlp");
	}
	if !ff_ok {
		missing.push("ffmpeg");
	}
	let msg_text = format!(
		"Tubex requires yt-dlp and ffmpeg to function.\nThe following were not found in your configuration or system PATH:\n\n- {}\n\nWould you like Tubex to download and configure them automatically?",
		missing.join("\n- ")
	);
	let msg = StaticText::builder(&dialog).with_label(&msg_text).build();
	sizer.add(&msg, 1, SizerFlag::Expand | SizerFlag::All, 10);
	let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let download_btn = Button::builder(&dialog).with_label("Download (Auto)").build();
	let settings_btn = Button::builder(&dialog).with_label("Configure Manually").build();
	btn_sizer.add_stretch_spacer(1);
	btn_sizer.add(&download_btn, 0, SizerFlag::All, 5);
	btn_sizer.add(&settings_btn, 0, SizerFlag::All, 5);
	btn_sizer.add_stretch_spacer(1);
	sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
	let d_dl = dialog;
	download_btn.on_click(move |_| d_dl.end_modal(2));
	let d_set = dialog;
	settings_btn.on_click(move |_| d_set.end_modal(1));
	dialog.set_sizer(sizer, true);
	dialog.centre();
	dialog.show_modal()
}

fn show_download_progress(parent: &Window) -> (Dialog, Gauge) {
	let dialog = Dialog::builder(parent, "Downloading Tools").with_size(300, 100).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();
	let gauge = Gauge::builder(&dialog).with_range(100).build();
	sizer.add(&gauge, 1, SizerFlag::Expand | SizerFlag::All, 10);
	dialog.set_sizer(sizer, true);
	dialog.centre();
	dialog.show(true);
	(dialog, gauge)
}
