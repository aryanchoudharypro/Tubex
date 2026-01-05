use std::{
	io::{BufRead, BufReader},
	os::windows::process::CommandExt,
	process::{Command, Stdio},
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, Ordering},
		mpsc,
	},
	thread,
};

use wxdragon::{
	ListColumnFormat, ListCtrlStyle, ListItemState, ListNextItemFlag, Orientation, PanelStyle, clipboard,
	prelude::*,
	widgets::{Choice, ListCtrl, Panel},
};

use crate::{
	config::ConfigManager,
	events::AppEvent,
	options_dialog::{show_channel_action_dialog, show_selection_dialog},
	video_info::VideoInfo,
};

const CREATE_NO_WINDOW: u32 = 0x08000000;

enum SearchEvent {
	Result(Vec<VideoInfo>, bool),
	PlaylistsFetched(Vec<(String, String)>),
	ReleasesFetched(Vec<(String, String)>),
	Error(String),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum SearchMode {
	Video,
	Channel,
	Playlist,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ChannelTab {
	Videos,
	Playlists,
	Releases,
}

struct SearchState {
	mode: SearchMode,
	query: String,
	offset: u32,
	is_channel_view: bool,
	channel_url: String,
	channel_tab: ChannelTab,
	results: Vec<VideoInfo>,
	auto_load: bool,
}

pub struct SearchTabContext {
	pub selected_videos: Arc<Mutex<Vec<VideoInfo>>>,
}

fn create_multi_select_list(parent: &Panel) -> ListCtrl {
	let list = ListCtrl::builder(parent).with_style(ListCtrlStyle::Report).build();
	list.insert_column(0, "Results", ListColumnFormat::Left, 800);
	list
}

pub fn create_search_tab(
	parent: &impl WxWidget,
	config_manager: Arc<Mutex<ConfigManager>>,
	tx_app: mpsc::Sender<AppEvent>,
) -> (Panel, SearchTabContext) {
	let yt_dlp_path = config_manager.lock().unwrap().get_yt_dlp_path();
	let panel = Panel::builder(parent).with_style(PanelStyle::TabTraversal).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();

	let top_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let back_btn = Button::builder(&panel).with_label("Back").build();
	back_btn.show(false);
	let search_label = StaticText::builder(&panel).with_label("Search:").build();
	let search_text = TextCtrl::builder(&panel).with_style(wxdragon::TextCtrlStyle::ProcessEnter).build();
	let mode_label = StaticText::builder(&panel).with_label("Search for:").build();
	let mode_choice =
		Choice::builder(&panel).with_choices(vec!["Videos".into(), "Channels".into(), "Playlists".into()]).build();
	mode_choice.set_selection(0);
	let search_btn = Button::builder(&panel).with_label("Search").build();

	top_sizer.add(&back_btn, 0, SizerFlag::All | SizerFlag::AlignCenterVertical, 5);
	top_sizer.add(&search_label, 0, SizerFlag::All | SizerFlag::AlignCenterVertical, 5);
	top_sizer.add(&search_text, 1, SizerFlag::All | SizerFlag::Expand, 5);
	top_sizer.add(&mode_label, 0, SizerFlag::All | SizerFlag::AlignCenterVertical, 5);
	top_sizer.add(&mode_choice, 0, SizerFlag::All | SizerFlag::AlignCenterVertical, 5);
	top_sizer.add(&search_btn, 0, SizerFlag::All | SizerFlag::AlignCenterVertical, 5);
	sizer.add_sizer(&top_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	let results_list = create_multi_select_list(&panel);
	sizer.add(&results_list, 1, SizerFlag::Expand | SizerFlag::All, 5);

	let channel_list = create_multi_select_list(&panel);
	channel_list.show(false);
	sizer.add(&channel_list, 1, SizerFlag::Expand | SizerFlag::All, 5);

	let bottom_sizer = BoxSizer::builder(Orientation::Horizontal).build();

	let load_more_btn = Button::builder(&panel).with_label("Load More...").build();
	let load_all_btn = Button::builder(&panel).with_label("Load All").build();
	let select_all_btn = Button::builder(&panel).with_label("Select All").build();
	let copy_url_btn = Button::builder(&panel).with_label("Copy URL").build();
	copy_url_btn.enable(false);
	let open_channel_btn = Button::builder(&panel).with_label("Open Channel").build();
	open_channel_btn.enable(false);
	bottom_sizer.add(&load_more_btn, 0, SizerFlag::All, 5);
	bottom_sizer.add(&load_all_btn, 0, SizerFlag::All, 5);
	bottom_sizer.add(&select_all_btn, 0, SizerFlag::All, 5);
	bottom_sizer.add(&copy_url_btn, 0, SizerFlag::All, 5);
	bottom_sizer.add_stretch_spacer(1);
	bottom_sizer.add(&open_channel_btn, 0, SizerFlag::All, 5);
	sizer.add_sizer(&bottom_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
	let status_text = StaticText::builder(&panel).with_label("Ready").build();
	sizer.add(&status_text, 0, SizerFlag::All | SizerFlag::Expand, 5);
	panel.set_sizer(sizer, true);

	let state = Arc::new(Mutex::new(SearchState {
		mode: SearchMode::Video,
		query: "".into(),
		offset: 1,
		is_channel_view: false,
		channel_url: "".into(),
		channel_tab: ChannelTab::Videos,
		results: Vec::new(),
		auto_load: false,
	}));
	let selected_videos = Arc::new(Mutex::new(Vec::new()));
	let (tx, rx) = mpsc::channel::<SearchEvent>();
	let timer = Box::leak(Box::new(Timer::new(&panel)));
	let status_clone = status_text;
	let state_rx = state.clone();

	let yt_path_search = yt_dlp_path.clone();
	let tx_search = tx.clone();
	let run_search = move |query: String,
	                       mode: SearchMode,
	                       channel_url: Option<String>,
	                       tab: Option<ChannelTab>,
	                       start: u32,
	                       append: bool| {
		let tx = tx_search.clone();
		let yt_path = yt_path_search.clone();
		thread::spawn(move || {
			let mut cmd = Command::new(&yt_path);
			cmd.creation_flags(CREATE_NO_WINDOW);
			cmd.arg("--dump-json").arg("--flat-playlist").arg("--skip-download");
			if start > 1 {
				cmd.arg("--playlist-start").arg(start.to_string());
			}
			cmd.arg("--playlist-end").arg((start + 19).to_string());

			if let Some(mut url) = channel_url {
				let suffix = match tab {
					Some(ChannelTab::Videos) => "/videos",
					Some(ChannelTab::Playlists) => "/playlists",
					Some(ChannelTab::Releases) => "/releases",
					_ => "",
				};
				if url.ends_with('/') {
					url.pop();
				}
				url.push_str(suffix);
				cmd.arg(&url);
			} else {
				match mode {
					SearchMode::Video => {
						cmd.arg(format!("ytsearch{}:{}", start + 19, query));
					}
					SearchMode::Channel => {
						cmd.arg(format!(
							"https://www.youtube.com/results?search_query={}&sp=EgIQAg%3D%3D",
							urlencoding::encode(&query)
						));
					}
					SearchMode::Playlist => {
						cmd.arg(format!(
							"https://www.youtube.com/results?search_query={}&sp=EgIQAw%3D%3D",
							urlencoding::encode(&query)
						));
					}
				}
			}
			cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
			if let Ok(mut child) = cmd.spawn() {
				let stdout = child.stdout.take();
				let mut videos = Vec::new();
				if let Some(out) = stdout {
					for line in BufReader::new(out).lines().map_while(Result::ok) {
						if let Ok(info) = serde_json::from_str::<VideoInfo>(&line) {
							videos.push(info);
						}
					}
				}
				let _ = tx.send(SearchEvent::Result(videos, append));
			} else {
				let _ = tx.send(SearchEvent::Error("Failed to run yt-dlp".into()));
			}
		});
	};

	let yt_path_list = yt_dlp_path.clone();
	let tx_list = tx.clone();
	let run_fetch_list = move |url: String, is_releases: bool| {
		let tx = tx_list.clone();
		let yt_path = yt_path_list.clone();
		thread::spawn(move || {
			let mut cmd = Command::new(&yt_path);
			cmd.creation_flags(CREATE_NO_WINDOW);
			cmd.arg("--flat-playlist").arg("--print").arg("%(title)s:::%(url)s");
			let mut full_url = url.clone();
			if full_url.ends_with('/') {
				full_url.pop();
			}
			full_url.push_str(if is_releases { "/releases" } else { "/playlists" });
			cmd.arg(&full_url);

			cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
			if let Ok(mut child) = cmd.spawn() {
				let stdout = child.stdout.take();
				let mut items = Vec::new();
				if let Some(out) = stdout {
					for line in BufReader::new(out).lines().map_while(Result::ok) {
						if let Some((title, url)) = line.split_once(":::") {
							items.push((title.to_string(), url.to_string()));
						}
					}
				}
				if is_releases {
					let _ = tx.send(SearchEvent::ReleasesFetched(items));
				} else {
					let _ = tx.send(SearchEvent::PlaylistsFetched(items));
				}
			} else {
				let _ = tx.send(SearchEvent::Error("Failed to fetch list".into()));
			}
		});
	};

	let rs_timer = run_search.clone();
	let results_list_clone = results_list;
	let channel_list_clone = channel_list;
	let panel_clone = panel;
	let tx_app_clone = tx_app.clone();

	timer.start(100, false);
	timer.on_tick(move |_| {
		while let Ok(event) = rx.try_recv() {
			match event {
				SearchEvent::Result(videos, append) => {
					let mut s = state_rx.lock().expect("Search state lock failed");
					let list_to_update = if s.is_channel_view { &channel_list_clone } else { &results_list_clone };
					if !append {
						list_to_update.delete_all_items();
						s.results.clear();
					}
					let got_results = !videos.is_empty();
					for v in videos {
						let mut label = String::new();
						if let Some(t) = &v.result_type
							&& t == "url" && let Some(u) = &v.url
						{
							if u.contains("/channel/") || u.contains("/@") {
								label.push_str("[Channel] ");
							} else if u.contains("playlist") {
								label.push_str("[Playlist] ");
							}
						}
						label.push_str(&v.title);
						if let Some(d) = v.duration {
							label.push_str(&format!(" [{}]", format_duration(d)));
						} else if v.playlist_count.is_some() {
							label.push_str(&format!(" ({} items)", v.playlist_count.unwrap_or(0)));
						}
						if let Some(u) = &v.uploader {
							label.push_str(&format!(" - {}", u));
						}
						if let Some(vc) = v.view_count {
							label.push_str(&format!(" - {}", format_views(vc)));
						}
						let index = list_to_update.get_item_count() as i64;
						list_to_update.insert_item(index, &label, None);
						s.results.push(v);
					}
					status_clone.set_label(&format!("Showing {} items", list_to_update.get_item_count()));

					if s.auto_load {
						if got_results {
							s.offset += 20;
							let (q, m, off, is_ch, c_url, c_tab) = (
								s.query.clone(),
								s.mode,
								s.offset,
								s.is_channel_view,
								s.channel_url.clone(),
								s.channel_tab,
							);
							rs_timer(
								q,
								m,
								if is_ch { Some(c_url) } else { None },
								if is_ch { Some(c_tab) } else { None },
								off,
								true,
							);
						} else {
							s.auto_load = false;
							status_clone.set_label("Finished loading all items.");
						}
					}
				}
				SearchEvent::PlaylistsFetched(items) | SearchEvent::ReleasesFetched(items) => {
					let titles: Vec<String> = items.iter().map(|(t, _)| t.clone()).collect();
					if let Some(indices) = show_selection_dialog(&panel_clone, "Select Content", &titles, true) {
						for idx in indices {
							if let Some((_, u)) = items.get(idx) {
								let _ = tx_app_clone.send(AppEvent::RequestFetch(u.clone()));
							}
						}
					}
				}
				SearchEvent::Error(e) => {
					let mut s = state_rx.lock().unwrap();
					s.auto_load = false;
					status_clone.set_label(&format!("Error: {}", e));
				}
			}
		}
	});

	let panel_layout = panel;
	let results_list_ui = results_list;
	let channel_list_ui = channel_list;
	let back_btn_ui = back_btn;
	let search_text_ui = search_text;
	let mode_choice_ui = mode_choice;
	let search_label_ui = search_label;
	let update_ui = move |is_channel: bool| {
		back_btn_ui.show(is_channel);
		search_text_ui.show(!is_channel);
		mode_choice_ui.show(!is_channel);
		search_label_ui.show(!is_channel);
		results_list_ui.show(!is_channel);
		channel_list_ui.show(is_channel);
		panel_layout.layout();
	};

	let rs_btn = run_search.clone();
	let state_btn = state.clone();
	let txt_btn = search_text;
	let mode_btn = mode_choice;
	let ui_search = update_ui;
	search_btn.on_click(move |_| {
		let q = txt_btn.get_value();
		if q.is_empty() {
			return;
		}
		let m_idx = mode_btn.get_selection().unwrap_or(0);
		let mode = match m_idx {
			1 => SearchMode::Channel,
			2 => SearchMode::Playlist,
			_ => SearchMode::Video,
		};
		let mut s = state_btn.lock().unwrap();
		s.mode = mode;
		s.query = q.clone();
		s.offset = 1;
		s.is_channel_view = false;
		s.auto_load = false;
		ui_search(false);
		rs_btn(q, mode, None, None, 1, false);
	});
	let batch_flag = Arc::new(AtomicBool::new(false));
	let batch_flag_handler = batch_flag.clone();
	let state_handler_base = state.clone();
	let sel_vid_handler_base = selected_videos.clone();

	let make_selection_handler = move |list: ListCtrl| {
		let ch_btn_sel = open_channel_btn;
		let cp_btn_sel = copy_url_btn;
		let state_sel = state_handler_base.clone();
		let sel_vid_clone = sel_vid_handler_base.clone();
		let list_clone_for_handler = list;
		let bf = batch_flag_handler.clone();

		let update_logic = move || {
			let mut selections = Vec::new();
			let mut item = list_clone_for_handler.get_next_item(-1, ListNextItemFlag::All, ListItemState::Selected);
			while item != -1 {
				selections.push(item);
				item =
					list_clone_for_handler.get_next_item(item as i64, ListNextItemFlag::All, ListItemState::Selected);
			}
			ch_btn_sel.enable(!selections.is_empty());
			cp_btn_sel.enable(!selections.is_empty());
			if let Ok(mut sv) = sel_vid_clone.lock() {
				sv.clear();
				if let Ok(s) = state_sel.lock() {
					for i in selections {
						if let Some(v) = s.results.get(i as usize) {
							sv.push(v.clone());
						}
					}
				}
			}
		};

		let logic_sel = update_logic.clone();
		let logic_desel = update_logic.clone();
		let bf_sel = bf.clone();
		let bf_desel = bf.clone();

		list.on_item_selected(move |_| {
			if !bf_sel.load(Ordering::Relaxed) {
				logic_sel();
			}
		});
		list.on_item_deselected(move |_| {
			if !bf_desel.load(Ordering::Relaxed) {
				logic_desel();
			}
		});
	};
	make_selection_handler(results_list);
	make_selection_handler(channel_list);

	let rf_ch = run_fetch_list.clone();
	let state_ch = state.clone();
	let ui_ch = update_ui;
	let panel_ch = panel;
	let tx_app_ch = tx_app.clone();

	let open_channel_logic = move |url: String| {
		if let Some(choice) = show_channel_action_dialog(&panel_ch) {
			match choice.as_str() {
				"Videos" => {
					let mut u = url.clone();
					if u.ends_with('/') {
						u.pop();
					}
					u.push_str("/videos");
					let _ = tx_app_ch.send(AppEvent::RequestFetch(u));
				}
				"Shorts" => {
					let mut u = url.clone();
					if u.ends_with('/') {
						u.pop();
					}
					u.push_str("/shorts");
					let _ = tx_app_ch.send(AppEvent::RequestFetch(u));
				}
				"Live" => {
					let mut u = url.clone();
					if u.ends_with('/') {
						u.pop();
					}
					u.push_str("/streams");
					let _ = tx_app_ch.send(AppEvent::RequestFetch(u));
				}
				"Playlists" => {
					let mut s = state_ch.lock().unwrap();
					s.is_channel_view = true;
					s.channel_url = url.clone();
					s.offset = 1;
					ui_ch(true);
					s.channel_tab = ChannelTab::Playlists;
					drop(s);
					rf_ch(url, false);
				}
				"Releases" => {
					let mut s = state_ch.lock().unwrap();
					s.is_channel_view = true;
					s.channel_url = url.clone();
					s.offset = 1;
					ui_ch(true);
					s.channel_tab = ChannelTab::Releases;
					drop(s);
					rf_ch(url, true);
				}
				_ => {}
			}
		}
	};

	let sel_vids_copy = selected_videos.clone();
	copy_url_btn.on_click(move |_| {
		if let Ok(sv) = sel_vids_copy.lock() {
			let urls: Vec<String> = sv.iter().filter_map(|v| v.webpage_url.clone().or(v.url.clone())).collect();
			if !urls.is_empty() {
				let cb = clipboard::Clipboard::get();
				cb.set_text(&urls.join("\n"));
			}
		}
	});

	let open_logic_btn = open_channel_logic.clone();
	let state_btn_ch = state.clone();
	open_channel_btn.on_click(move |_| {
		let list = results_list;
		let item = list.get_next_item(-1, ListNextItemFlag::All, ListItemState::Selected);
		if item != -1
			&& let Ok(s) = state_btn_ch.lock()
			&& let Some(v) = s.results.get(item as usize)
		{
			let target = v.channel_url.clone().or_else(|| {
				if v.url.as_ref().is_some_and(|u| u.contains("/channel/") || u.contains("/@")) {
					v.url.clone()
				} else {
					None
				}
			});
			if let Some(u) = target {
				drop(s);
				open_logic_btn(u);
			}
		}
	});

	let state_back = state.clone();
	let ui_back = update_ui;
	let rs_back = run_search.clone();
	let list_clear_res = results_list;
	let list_clear_ch = channel_list;
	let sel_clear = selected_videos.clone();

	back_btn.on_click(move |_| {
		let mut s = state_back.lock().unwrap();
		s.is_channel_view = false;

		list_clear_res.delete_all_items();
		list_clear_ch.delete_all_items();
		if let Ok(mut sc) = sel_clear.lock() {
			sc.clear();
		}

		let (q, m) = (s.query.clone(), s.mode);
		ui_back(false);
		rs_back(q, m, None, None, 1, false);
	});

	let rs_more = run_search.clone();
	let state_more = state.clone();
	load_more_btn.on_click(move |_| {
		let mut s = state_more.lock().unwrap();
		s.offset += 20;
		s.auto_load = false;
		let (q, m, off, is_ch, c_url, c_tab) =
			(s.query.clone(), s.mode, s.offset, s.is_channel_view, s.channel_url.clone(), s.channel_tab);
		rs_more(q, m, if is_ch { Some(c_url) } else { None }, if is_ch { Some(c_tab) } else { None }, off, true);
	});

	let rs_all = run_search.clone();
	let state_all = state.clone();
	load_all_btn.on_click(move |_| {
		let mut s = state_all.lock().unwrap();
		s.auto_load = true;
		s.offset += 20;
		let (q, m, off, is_ch, c_url, c_tab) =
			(s.query.clone(), s.mode, s.offset, s.is_channel_view, s.channel_url.clone(), s.channel_tab);
		rs_all(q, m, if is_ch { Some(c_url) } else { None }, if is_ch { Some(c_tab) } else { None }, off, true);
	});

	let state_sel_all = state.clone();
	let list_res_sel = results_list;
	let list_ch_sel = channel_list;
	let batch_flag_all = batch_flag.clone();
	let sel_vid_all = selected_videos.clone();
	let ch_btn_all = open_channel_btn;
	let cp_btn_all = copy_url_btn;

	select_all_btn.on_click(move |_| {
		let s = state_sel_all.lock().unwrap();
		let list = if s.is_channel_view { &list_ch_sel } else { &list_res_sel };

		let count = list.get_item_count();
		if count == 0 {
			return;
		}

		batch_flag_all.store(true, Ordering::Relaxed);
		for i in 0..count {
			list.set_item_state(i as i64, ListItemState::Selected, ListItemState::Selected);
		}
		batch_flag_all.store(false, Ordering::Relaxed);

		if let Ok(mut sv) = sel_vid_all.lock() {
			sv.clear();
			sv.extend_from_slice(&s.results);
		}
		ch_btn_all.enable(true);
		cp_btn_all.enable(true);
	});

	(panel, SearchTabContext { selected_videos })
}

fn format_duration(seconds: f64) -> String {
	let s = seconds as u64;
	let h = s / 3600;
	let m = (s % 3600) / 60;
	let s = s % 60;
	if h > 0 { format!("{}:{:02}:{:02}", h, m, s) } else { format!("{}:{:02}", m, s) }
}

fn format_views(count: u64) -> String {
	if count >= 1_000_000_000 {
		format!("{:.1}B views", count as f64 / 1_000_000_000.0)
	} else if count >= 1_000_000 {
		format!("{:.1}M views", count as f64 / 1_000_000.0)
	} else if count >= 1_000 {
		format!("{:.1}K views", count as f64 / 1_000.0)
	} else {
		format!("{} views", count)
	}
}
