use wxdragon::{
	ListColumnFormat, ListCtrlStyle, ListItemState, ListNextItemFlag, Orientation, PanelStyle,
	prelude::*,
	widgets::{CheckBox, ListCtrl, Notebook, Panel},
};

use crate::video_info::{Format, VideoInfo};

const RET_OK: i32 = 1;
const RET_CANCEL: i32 = 0;

#[derive(Debug, Clone)]
pub enum DownloadMode {
	Single { video_format: Option<String>, audio_formats: Vec<String> },
	Playlist { indices: Vec<usize>, video_format: Option<String>, audio_formats: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct DownloadOptions {
	pub mode: DownloadMode,
	pub add_chapters: bool,
	pub multi_audio: bool,
	pub preferred_languages: Vec<String>,
}

fn format_label(f: &Format) -> String {
	let res = f.width.zip(f.height).map(|(w, h)| format!("{}x{}", w, h)).unwrap_or_else(|| "audio".to_string());
	format!(
		"[{}] {} {} [{}] - {}",
		f.format_id,
		f.ext.as_deref().unwrap_or("?"),
		res,
		f.language.as_deref().unwrap_or("?"),
		f.format_note.as_deref().unwrap_or("")
	)
}

pub fn show_options_dialog(parent: &impl WxWidget, info: &VideoInfo) -> Option<DownloadOptions> {
	let dialog = Dialog::builder(parent, "Download Options").with_size(600, 500).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	main_sizer.add(
		&StaticText::builder(&dialog).with_label(&format!("Title: {}", info.title)).build(),
		0,
		SizerFlag::All | SizerFlag::Expand,
		10,
	);

	let options_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let chk_chapters = CheckBox::builder(&dialog).with_label("Add Chapters (--embed-chapters)").build();
	let chk_multi_audio =
		CheckBox::builder(&dialog).with_label("Allow Multiple Audio Streams (--audio-multistreams)").build();
	options_sizer.add(&chk_chapters, 0, SizerFlag::All, 5);
	options_sizer.add(&chk_multi_audio, 0, SizerFlag::All, 5);
	main_sizer.add_sizer(&options_sizer, 0, SizerFlag::All | SizerFlag::Expand, 5);

	let notebook = Notebook::builder(&dialog).build();
	main_sizer.add(&notebook, 1, SizerFlag::Expand | SizerFlag::All, 5);

	let video_panel = Panel::builder(&notebook).with_style(PanelStyle::TabTraversal).build();
	let video_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let mut sorted_video = info.get_video_formats().clone();
	sorted_video.sort_by(|a, b| b.height.unwrap_or(0).cmp(&a.height.unwrap_or(0)));

	let video_list =
		ListCtrl::builder(&video_panel).with_style(ListCtrlStyle::Report | ListCtrlStyle::SingleSel).build();
	video_list.insert_column(0, "Format", ListColumnFormat::Left, 400);
	for (i, fmt) in sorted_video.iter().enumerate() {
		video_list.insert_item(i as i64, &format_label(fmt), None);
	}
	if !sorted_video.is_empty() {
		video_list.set_item_state(0, ListItemState::Selected, ListItemState::Selected);
	}
	video_sizer.add(&video_list, 1, SizerFlag::Expand | SizerFlag::All, 0);
	video_panel.set_sizer(video_sizer, true);
	notebook.add_page(&video_panel, "Video", true, None);

	let audio_panel = Panel::builder(&notebook).with_style(PanelStyle::TabTraversal).build();
	let audio_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let mut sorted_audio = info.get_audio_formats().clone();
	sorted_audio.sort_by(|a, b| b.filesize.unwrap_or(0).cmp(&a.filesize.unwrap_or(0)));

	let audio_list = ListCtrl::builder(&audio_panel).with_style(ListCtrlStyle::Report).build();
	audio_list.insert_column(0, "Format", ListColumnFormat::Left, 400);
	for (i, fmt) in sorted_audio.iter().enumerate() {
		audio_list.insert_item(i as i64, &format_label(fmt), None);
	}
	if !sorted_audio.is_empty() {
		audio_list.set_item_state(0, ListItemState::Selected, ListItemState::Selected);
	}
	audio_sizer.add(&audio_list, 1, SizerFlag::Expand | SizerFlag::All, 0);
	audio_panel.set_sizer(audio_sizer, true);
	notebook.add_page(&audio_panel, "Audio", false, None);

	let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_btn = Button::builder(&dialog).with_label("Download").build();
	let cancel_btn = Button::builder(&dialog).with_label("Cancel").build();
	btn_sizer.add_stretch_spacer(1);
	btn_sizer.add(&ok_btn, 0, SizerFlag::All, 5);
	btn_sizer.add(&cancel_btn, 0, SizerFlag::All, 5);
	main_sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand | SizerFlag::All, 10);

	let d_ok = dialog;
	ok_btn.on_click(move |_| d_ok.end_modal(RET_OK));
	let d_cancel = dialog;
	cancel_btn.on_click(move |_| d_cancel.end_modal(RET_CANCEL));

	dialog.set_sizer(main_sizer, true);
	dialog.centre();

	(dialog.show_modal() == RET_OK).then(|| {
		let item = video_list.get_next_item(-1, ListNextItemFlag::All, ListItemState::Selected);
		let video_format = (item != -1).then(|| sorted_video.get(item as usize).map(|f| f.format_id.clone())).flatten();

		let mut audio_formats = Vec::new();
		let mut preferred_languages = Vec::new();
		let mut item = audio_list.get_next_item(-1, ListNextItemFlag::All, ListItemState::Selected);
		while item != -1 {
			if let Some(f) = sorted_audio.get(item as usize) {
				audio_formats.push(f.format_id.clone());
				if let Some(l) = &f.language {
					preferred_languages.push(l.clone());
				}
			}
			item = audio_list.get_next_item(item as i64, ListNextItemFlag::All, ListItemState::Selected);
		}

		DownloadOptions {
			mode: DownloadMode::Single { video_format, audio_formats },
			add_chapters: chk_chapters.get_value(),
			multi_audio: chk_multi_audio.get_value(),
			preferred_languages,
		}
	})
}

pub fn show_playlist_dialog(parent: &impl WxWidget, videos: &[VideoInfo]) -> Option<DownloadOptions> {
	if videos.is_empty() {
		None
	} else {
		let ref_video = &videos[0];
		let dialog = Dialog::builder(parent, "Playlist Download Options").with_size(800, 700).build();
		let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

		main_sizer.add(
			&StaticText::builder(&dialog)
				.with_label(&format!("Found {} videos. Select items to download:", videos.len()))
				.build(),
			0,
			SizerFlag::All | SizerFlag::Expand,
			5,
		);

		let list_ctrl = ListCtrl::builder(&dialog).with_style(ListCtrlStyle::Report).build();
		list_ctrl.insert_column(0, "Video", ListColumnFormat::Left, 550);
		for (i, v) in videos.iter().enumerate() {
			list_ctrl.insert_item(i as i64, &format!("{}: {}", i + 1, v.title), None);
			list_ctrl.set_item_state(i as i64, ListItemState::Selected, ListItemState::Selected);
		}
		main_sizer.add(&list_ctrl, 2, SizerFlag::Expand | SizerFlag::All, 5);

		let sel_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let select_all_btn = Button::builder(&dialog).with_label("Select All").build();
		let select_none_btn = Button::builder(&dialog).with_label("Select None").build();
		sel_sizer.add(&select_all_btn, 0, SizerFlag::All, 2);
		sel_sizer.add(&select_none_btn, 0, SizerFlag::All, 2);
		main_sizer.add_sizer(&sel_sizer, 0, SizerFlag::All, 2);

		main_sizer.add(
			&StaticText::builder(&dialog).with_label("Select Format (Based on 1st video):").build(),
			0,
			SizerFlag::All | SizerFlag::Top,
			10,
		);

		let notebook = Notebook::builder(&dialog).build();
		main_sizer.add(&notebook, 3, SizerFlag::Expand | SizerFlag::All, 5);

		let video_panel = Panel::builder(&notebook).with_style(PanelStyle::TabTraversal).build();
		let video_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let mut sorted_video = ref_video.get_video_formats().clone();
		sorted_video.sort_by(|a, b| b.height.unwrap_or(0).cmp(&a.height.unwrap_or(0)));

		let video_list =
			ListCtrl::builder(&video_panel).with_style(ListCtrlStyle::Report | ListCtrlStyle::SingleSel).build();
		video_list.insert_column(0, "Format", ListColumnFormat::Left, 400);
		for (i, fmt) in sorted_video.iter().enumerate() {
			video_list.insert_item(i as i64, &format_label(fmt), None);
		}
		if !sorted_video.is_empty() {
			video_list.set_item_state(0, ListItemState::Selected, ListItemState::Selected);
		}
		video_sizer.add(&video_list, 1, SizerFlag::Expand | SizerFlag::All, 0);
		video_panel.set_sizer(video_sizer, true);
		notebook.add_page(&video_panel, "Video", true, None);

		let audio_panel = Panel::builder(&notebook).with_style(PanelStyle::TabTraversal).build();
		let audio_sizer = BoxSizer::builder(Orientation::Vertical).build();
		let mut sorted_audio = ref_video.get_audio_formats().clone();
		sorted_audio.sort_by(|a, b| b.filesize.unwrap_or(0).cmp(&a.filesize.unwrap_or(0)));

		let audio_list = ListCtrl::builder(&audio_panel).with_style(ListCtrlStyle::Report).build();
		audio_list.insert_column(0, "Format", ListColumnFormat::Left, 400);
		for (i, fmt) in sorted_audio.iter().enumerate() {
			audio_list.insert_item(i as i64, &format_label(fmt), None);
		}
		if !sorted_audio.is_empty() {
			audio_list.set_item_state(0, ListItemState::Selected, ListItemState::Selected);
		}
		audio_sizer.add(&audio_list, 1, SizerFlag::Expand | SizerFlag::All, 0);
		audio_panel.set_sizer(audio_sizer, true);
		notebook.add_page(&audio_panel, "Audio", false, None);

		let options_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let chk_chapters = CheckBox::builder(&dialog).with_label("Add Chapters").build();
		chk_chapters.set_value(true);
		let chk_multi_audio = CheckBox::builder(&dialog).with_label("Multi-Audio").build();
		options_sizer.add(&chk_chapters, 0, SizerFlag::All, 5);
		options_sizer.add(&chk_multi_audio, 0, SizerFlag::All, 5);
		main_sizer.add_sizer(&options_sizer, 0, SizerFlag::All, 5);

		let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let ok_btn = Button::builder(&dialog).with_label("Download").build();
		let cancel_btn = Button::builder(&dialog).with_label("Cancel").build();
		btn_sizer.add_stretch_spacer(1);
		btn_sizer.add(&ok_btn, 0, SizerFlag::All, 5);
		btn_sizer.add(&cancel_btn, 0, SizerFlag::All, 5);
		main_sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand | SizerFlag::All, 10);

		let list_sel_all = list_ctrl;
		let count = videos.len();
		select_all_btn.on_click(move |_| {
			(0..count).for_each(|i| {
				list_sel_all.set_item_state(i as i64, ListItemState::Selected, ListItemState::Selected);
			})
		});

		let list_sel_none = list_ctrl;
		select_none_btn.on_click(move |_| {
			(0..count).for_each(|i| {
				list_sel_none.set_item_state(i as i64, ListItemState::None, ListItemState::Selected);
			})
		});

		let d_ok = dialog;
		ok_btn.on_click(move |_| d_ok.end_modal(RET_OK));
		let d_cancel = dialog;
		cancel_btn.on_click(move |_| d_cancel.end_modal(RET_CANCEL));

		dialog.set_sizer(main_sizer, true);
		dialog.centre();

		(dialog.show_modal() == RET_OK).then(|| {
			let mut indices = Vec::new();
			let mut item = list_ctrl.get_next_item(-1, ListNextItemFlag::All, ListItemState::Selected);
			while item != -1 {
				indices.push((item + 1) as usize);
				item = list_ctrl.get_next_item(item as i64, ListNextItemFlag::All, ListItemState::Selected);
			}

			let v_item = video_list.get_next_item(-1, ListNextItemFlag::All, ListItemState::Selected);
			let video_format =
				(v_item != -1).then(|| sorted_video.get(v_item as usize).map(|f| f.format_id.clone())).flatten();

			let mut audio_formats = Vec::new();
			let mut preferred_languages = Vec::new();
			let mut a_item = audio_list.get_next_item(-1, ListNextItemFlag::All, ListItemState::Selected);
			while a_item != -1 {
				if let Some(f) = sorted_audio.get(a_item as usize) {
					audio_formats.push(f.format_id.clone());
					if let Some(l) = &f.language {
						preferred_languages.push(l.clone());
					}
				}
				a_item = audio_list.get_next_item(a_item as i64, ListNextItemFlag::All, ListItemState::Selected);
			}

			DownloadOptions {
				mode: DownloadMode::Playlist { indices, video_format, audio_formats },
				add_chapters: chk_chapters.get_value(),
				multi_audio: chk_multi_audio.get_value(),
				preferred_languages,
			}
		})
	}
}

pub fn show_channel_action_dialog(parent: &impl WxWidget) -> Option<String> {
	let dialog = Dialog::builder(parent, "Select Channel Tab").with_size(300, 400).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();

	let list = ListCtrl::builder(&dialog).with_style(ListCtrlStyle::Report | ListCtrlStyle::SingleSel).build();
	list.insert_column(0, "Tab", ListColumnFormat::Left, 280);
	let options = ["Videos", "Shorts", "Live", "Playlists", "Releases"];
	for (i, opt) in options.iter().enumerate() {
		list.insert_item(i as i64, opt, None);
	}
	list.set_item_state(0, ListItemState::Selected, ListItemState::Selected);
	sizer.add(&list, 1, SizerFlag::Expand | SizerFlag::All, 5);

	let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_btn = Button::builder(&dialog).with_label("Open").build();
	let cancel_btn = Button::builder(&dialog).with_label("Cancel").build();
	btn_sizer.add_stretch_spacer(1);
	btn_sizer.add(&ok_btn, 0, SizerFlag::All, 5);
	btn_sizer.add(&cancel_btn, 0, SizerFlag::All, 5);
	sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	let d_ok = dialog;
	ok_btn.on_click(move |_| d_ok.end_modal(RET_OK));
	let d_cancel = dialog;
	cancel_btn.on_click(move |_| d_cancel.end_modal(RET_CANCEL));

	dialog.set_sizer(sizer, true);
	dialog.centre();

	if dialog.show_modal() == RET_OK {
		let item = list.get_next_item(-1, ListNextItemFlag::All, ListItemState::Selected);
		if item != -1 { Some(options[item as usize].to_string()) } else { None }
	} else {
		None
	}
}

pub fn show_selection_dialog(
	parent: &impl WxWidget,
	title: &str,
	items: &[String],
	allow_multiple: bool,
) -> Option<Vec<usize>> {
	let dialog = Dialog::builder(parent, title).with_size(500, 600).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();

	let list = ListCtrl::builder(&dialog)
		.with_style(if allow_multiple {
			ListCtrlStyle::Report
		} else {
			ListCtrlStyle::Report | ListCtrlStyle::SingleSel
		})
		.build();
	list.insert_column(0, "Item", ListColumnFormat::Left, 450);
	for (i, item) in items.iter().enumerate() {
		list.insert_item(i as i64, item, None);
	}
	if !items.is_empty() {
		list.set_item_state(0, ListItemState::Selected, ListItemState::Selected);
	}
	sizer.add(&list, 1, SizerFlag::Expand | SizerFlag::All, 5);

	if allow_multiple {
		let sel_sizer = BoxSizer::builder(Orientation::Horizontal).build();
		let select_all_btn = Button::builder(&dialog).with_label("Select All").build();
		let select_none_btn = Button::builder(&dialog).with_label("Select None").build();
		sel_sizer.add(&select_all_btn, 0, SizerFlag::All, 2);
		sel_sizer.add(&select_none_btn, 0, SizerFlag::All, 2);
		sizer.add_sizer(&sel_sizer, 0, SizerFlag::All, 2);

		let l_all = list;
		let count = items.len();
		select_all_btn.on_click(move |_| {
			(0..count).for_each(|i| {
				l_all.set_item_state(i as i64, ListItemState::Selected, ListItemState::Selected);
			})
		});
		let l_none = list;
		select_none_btn.on_click(move |_| {
			(0..count).for_each(|i| {
				l_none.set_item_state(i as i64, ListItemState::None, ListItemState::Selected);
			})
		});
	}

	let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_btn = Button::builder(&dialog).with_label("Select").build();
	let cancel_btn = Button::builder(&dialog).with_label("Cancel").build();
	btn_sizer.add_stretch_spacer(1);
	btn_sizer.add(&ok_btn, 0, SizerFlag::All, 5);
	btn_sizer.add(&cancel_btn, 0, SizerFlag::All, 5);
	sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	let d_ok = dialog;
	ok_btn.on_click(move |_| d_ok.end_modal(RET_OK));
	let d_cancel = dialog;
	cancel_btn.on_click(move |_| d_cancel.end_modal(RET_CANCEL));

	dialog.set_sizer(sizer, true);
	dialog.centre();

	if dialog.show_modal() == RET_OK {
		let mut indices = Vec::new();
		let mut item = list.get_next_item(-1, ListNextItemFlag::All, ListItemState::Selected);
		while item != -1 {
			indices.push(item as usize);
			item = list.get_next_item(item as i64, ListNextItemFlag::All, ListItemState::Selected);
		}
		Some(indices)
	} else {
		None
	}
}
