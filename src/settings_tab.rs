use std::sync::{Arc, Mutex};

use wxdragon::prelude::*;

use crate::config::ConfigManager;

pub fn create_settings_tab(parent: &Notebook, config_manager: Arc<Mutex<ConfigManager>>) -> Panel {
	let panel = Panel::builder(parent).with_style(wxdragon::PanelStyle::TabTraversal).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();

	let path_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	path_sizer.add(
		&StaticText::builder(&panel).with_label("Download Path:").build(),
		0,
		SizerFlag::AlignCenterVertical | SizerFlag::All,
		5,
	);
	let path_text = TextCtrl::builder(&panel).build();
	if let Some(p) = config_manager.lock().expect("Config manager lock failed").get_download_path() {
		path_text.set_value(&p)
	}
	path_sizer.add(&path_text, 1, SizerFlag::Expand | SizerFlag::All, 5);
	let browse_btn = Button::builder(&panel).with_label("Browse...").build();
	path_sizer.add(&browse_btn, 0, SizerFlag::All, 5);
	sizer.add_sizer(&path_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	let ytdlp_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	ytdlp_sizer.add(
		&StaticText::builder(&panel).with_label("yt-dlp Path:").build(),
		0,
		SizerFlag::AlignCenterVertical | SizerFlag::All,
		5,
	);
	let ytdlp_text = TextCtrl::builder(&panel).build();
	ytdlp_text.set_value(&config_manager.lock().expect("Config manager lock failed").get_yt_dlp_path());
	ytdlp_sizer.add(&ytdlp_text, 1, SizerFlag::Expand | SizerFlag::All, 5);
	let ytdlp_browse = Button::builder(&panel).with_label("Browse...").build();
	ytdlp_sizer.add(&ytdlp_browse, 0, SizerFlag::All, 5);
	let check_btn = Button::builder(&panel).with_label("Check Version").build();
	ytdlp_sizer.add(&check_btn, 0, SizerFlag::All, 5);
	sizer.add_sizer(&ytdlp_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	let channel_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	channel_sizer.add(
		&StaticText::builder(&panel).with_label("Update Channel:").build(),
		0,
		SizerFlag::AlignCenterVertical | SizerFlag::All,
		5,
	);
	let channel_choice =
		Choice::builder(&panel).with_choices(vec!["stable".into(), "nightly".into(), "master".into()]).build();
	channel_choice.set_selection(
		match config_manager.lock().expect("Config manager lock failed").get_update_channel().as_str() {
			"nightly" => 1,
			"master" => 2,
			_ => 0,
		},
	);
	channel_sizer.add(&channel_choice, 1, SizerFlag::Expand | SizerFlag::All, 5);
	sizer.add_sizer(&channel_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	let ffmpeg_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	ffmpeg_sizer.add(
		&StaticText::builder(&panel).with_label("FFmpeg Path:").build(),
		0,
		SizerFlag::AlignCenterVertical | SizerFlag::All,
		5,
	);
	let ffmpeg_text = TextCtrl::builder(&panel).build();
	ffmpeg_text.set_value(&config_manager.lock().expect("Config manager lock failed").get_ffmpeg_path());
	ffmpeg_sizer.add(&ffmpeg_text, 1, SizerFlag::Expand | SizerFlag::All, 5);
	let ffmpeg_browse = Button::builder(&panel).with_label("Browse...").build();
	ffmpeg_sizer.add(&ffmpeg_browse, 0, SizerFlag::All, 5);
	sizer.add_sizer(&ffmpeg_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	let flags_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	flags_sizer.add(
		&StaticText::builder(&panel).with_label("Global Flags:").build(),
		0,
		SizerFlag::AlignCenterVertical | SizerFlag::All,
		5,
	);
	let flags_text = TextCtrl::builder(&panel).build();
	flags_text.set_value(&config_manager.lock().expect("Config manager lock failed").get_global_flags());
	flags_sizer.add(&flags_text, 1, SizerFlag::Expand | SizerFlag::All, 5);
	sizer.add_sizer(&flags_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	let save_btn = Button::builder(&panel).with_label("Save Settings").build();
	sizer.add(&save_btn, 0, SizerFlag::All | SizerFlag::AlignRight, 10);
	panel.set_sizer(sizer, true);

	let panel_clone = panel;
	let path_text_clone = path_text;
	browse_btn.on_click(move |_| {
		let dialog =
			DirDialog::builder(&panel_clone, "Select Download Directory", &path_text_clone.get_value()).build();
		if dialog.show_modal() == wxdragon::id::ID_OK
			&& let Some(p) = dialog.get_path()
		{
			path_text_clone.set_value(&p)
		}
	});

	let panel_ytdlp = panel;
	let ytdlp_text_clone = ytdlp_text;
	ytdlp_browse.on_click(move |_| {
		let dialog = FileDialog::builder(&panel_ytdlp)
			.with_message("Select yt-dlp Executable")
			.with_wildcard("Executables (*.exe)|*.exe|All Files (*.*)|*.*")
			.build();
		if dialog.show_modal() == wxdragon::id::ID_OK
			&& let Some(p) = dialog.get_path()
		{
			ytdlp_text_clone.set_value(&p)
		}
	});

	let panel_ffmpeg = panel;
	let ffmpeg_text_clone = ffmpeg_text;
	ffmpeg_browse.on_click(move |_| {
		let dialog = FileDialog::builder(&panel_ffmpeg)
			.with_message("Select FFmpeg Executable")
			.with_wildcard("Executables (*.exe)|*.exe|All Files (*.*)|*.*")
			.build();
		if dialog.show_modal() == wxdragon::id::ID_OK
			&& let Some(p) = dialog.get_path()
		{
			ffmpeg_text_clone.set_value(&p)
		}
	});

	let ytdlp_text_check = ytdlp_text;
	let panel_check = panel;
	check_btn.on_click(move |_| {
		match std::process::Command::new(ytdlp_text_check.get_value()).arg("--version").output() {
			Ok(out) => {
				let msg = if out.status.success() {
					format!("Success! Version: {}", String::from_utf8_lossy(&out.stdout).trim())
				} else {
					format!("Error: {}", String::from_utf8_lossy(&out.stderr))
				};
				let _ = MessageDialog::builder(&panel_check, &msg, "yt-dlp Check").build().show_modal();
			}
			Err(e) => {
				let _ = MessageDialog::builder(&panel_check, &format!("Failed to execute: {}", e), "Error")
					.build()
					.show_modal();
			}
		}
	});

	let cfg_save = config_manager.clone();
	let path_save = path_text;
	let ytdlp_save = ytdlp_text;
	let channel_save = channel_choice;
	let ffmpeg_save = ffmpeg_text;
	let flags_save = flags_text;
	let panel_save = panel;

	save_btn.on_click(move |_| {
		let mut cfg = cfg_save.lock().expect("Config manager lock failed");
		cfg.set_download_path(&path_save.get_value());
		cfg.set_yt_dlp_path(&ytdlp_save.get_value());
		cfg.set_update_channel(match channel_save.get_selection() {
			Some(1) => "nightly",
			Some(2) => "master",
			_ => "stable",
		});
		cfg.set_ffmpeg_path(&ffmpeg_save.get_value());
		cfg.set_global_flags(&flags_save.get_value());
		cfg.flush();
		let _ = MessageDialog::builder(&panel_save, "Settings saved successfully.", "Info").build().show_modal();
	});

	panel
}
