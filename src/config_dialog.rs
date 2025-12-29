use std::sync::{Arc, Mutex};

use wxdragon::{prelude::*, window::Window};

use crate::config::{ConfigManager, CustomCommand};

const RET_OK: i32 = 1;
const RET_CANCEL: i32 = 0;

fn show_custom_message(parent: &Window, message: &str, caption: &str) -> i32 {
	let dialog = Dialog::builder(parent, caption).with_size(300, 150).build();
	let sizer = BoxSizer::builder(Orientation::Vertical).build();
	sizer.add(&StaticText::builder(&dialog).with_label(message).build(), 1, SizerFlag::Expand | SizerFlag::All, 10);
	let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_btn = Button::builder(&dialog).with_label("OK").build();
	btn_sizer.add_stretch_spacer(1);
	btn_sizer.add(&ok_btn, 0, SizerFlag::All, 5);
	sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
	let d_clone = dialog;
	ok_btn.on_click(move |_| d_clone.end_modal(RET_OK));
	dialog.set_sizer(sizer, true);
	dialog.centre();
	dialog.show_modal()
}

fn show_command_dialog(parent: &Window, title: &str, command: Option<&CustomCommand>) -> Option<CustomCommand> {
	let dialog = Dialog::builder(parent, title).with_size(400, 200).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();

	let name_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	name_sizer.add(
		&StaticText::builder(&dialog).with_label("Name:").build(),
		0,
		SizerFlag::AlignCenterVertical | SizerFlag::All,
		5,
	);
	let name_text = TextCtrl::builder(&dialog).build();
	name_sizer.add(&name_text, 1, SizerFlag::Expand | SizerFlag::All, 5);
	main_sizer.add_sizer(&name_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	let value_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	value_sizer.add(
		&StaticText::builder(&dialog).with_label("Value:").build(),
		0,
		SizerFlag::AlignCenterVertical | SizerFlag::All,
		5,
	);
	let value_text = TextCtrl::builder(&dialog).build();
	value_sizer.add(&value_text, 1, SizerFlag::Expand | SizerFlag::All, 5);
	main_sizer.add_sizer(&value_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	if let Some(cmd) = command {
		name_text.set_value(&cmd.name);
		value_text.set_value(&cmd.value);
	}

	let btn_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_btn = Button::builder(&dialog).with_label("OK").build();
	let cancel_btn = Button::builder(&dialog).with_label("Cancel").build();
	btn_sizer.add_stretch_spacer(1);
	btn_sizer.add(&ok_btn, 0, SizerFlag::All, 5);
	btn_sizer.add(&cancel_btn, 0, SizerFlag::All, 5);
	main_sizer.add_stretch_spacer(1);
	main_sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	dialog.set_sizer(main_sizer, true);
	dialog.centre();

	let d_ok = dialog;
	ok_btn.on_click(move |_| d_ok.end_modal(RET_OK));
	let d_cancel = dialog;
	cancel_btn.on_click(move |_| d_cancel.end_modal(RET_CANCEL));

	if dialog.show_modal() == RET_OK {
		let name = name_text.get_value();
		if name.trim().is_empty() {
			show_custom_message(&dialog, "The name cannot be empty.", "Error");
			None
		} else {
			Some(CustomCommand { name, value: value_text.get_value() })
		}
	} else {
		None
	}
}

pub fn show_config_dialog(parent: &Window, config_manager: Arc<Mutex<ConfigManager>>) {
	let dialog = Dialog::builder(parent, "Configure Custom Commands").with_size(600, 400).build();
	let main_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let content_sizer = BoxSizer::builder(Orientation::Horizontal).build();

	let commands: Vec<String> = config_manager
		.lock()
		.expect("Config manager lock failed")
		.get_commands()
		.iter()
		.map(|c| c.name.clone())
		.collect();
	let list_box = ListBox::builder(&dialog).with_choices(commands).build();
	content_sizer.add(&list_box, 1, SizerFlag::Expand | SizerFlag::All, 5);

	let btn_sizer = BoxSizer::builder(Orientation::Vertical).build();
	let add_btn = Button::builder(&dialog).with_label("Add...").build();
	let edit_btn = Button::builder(&dialog).with_label("Edit...").build();
	let remove_btn = Button::builder(&dialog).with_label("Remove").build();
	btn_sizer.add(&add_btn, 0, SizerFlag::All, 5);
	btn_sizer.add(&edit_btn, 0, SizerFlag::All, 5);
	btn_sizer.add(&remove_btn, 0, SizerFlag::All, 5);
	btn_sizer.add_stretch_spacer(1);
	content_sizer.add_sizer(&btn_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);
	main_sizer.add_sizer(&content_sizer, 1, SizerFlag::Expand | SizerFlag::All, 5);

	let std_sizer = BoxSizer::builder(Orientation::Horizontal).build();
	let ok_btn = Button::builder(&dialog).with_label("OK").build();
	let cancel_btn = Button::builder(&dialog).with_label("Cancel").build();
	std_sizer.add_stretch_spacer(1);
	std_sizer.add(&ok_btn, 0, SizerFlag::All, 5);
	std_sizer.add(&cancel_btn, 0, SizerFlag::All, 5);
	main_sizer.add_sizer(&std_sizer, 0, SizerFlag::Expand | SizerFlag::All, 5);

	let dlg = dialog;
	let cfg = Arc::clone(&config_manager);
	let lb_clone = list_box;
	add_btn.on_click(move |_| {
		if let Some(new_cmd) = show_command_dialog(&dlg, "Add Command", None) {
			let mut cfg = cfg.lock().expect("Config manager lock failed");
			let mut cmds = cfg.get_commands().to_vec();
			cmds.push(new_cmd.clone());
			cfg.set_commands(&cmds);
			lb_clone.append(&new_cmd.name);
		}
	});

	let dlg2 = dialog;
	let cfg2 = Arc::clone(&config_manager);
	let lb2 = list_box;
	edit_btn.on_click(move |_| {
		if let Some(sel) = lb2.get_selection() {
			let cmd_opt = cfg2.lock().expect("Config manager lock failed").get_commands().get(sel as usize).cloned();
			if let Some(cmd) = cmd_opt
				&& let Some(edited) = show_command_dialog(&dlg2, "Edit Command", Some(&cmd))
			{
				let mut cfg = cfg2.lock().expect("Config manager lock failed");
				let mut cmds = cfg.get_commands().to_vec();
				cmds[sel as usize] = edited.clone();
				cfg.set_commands(&cmds);
				lb2.delete(sel);
				lb2.append(&edited.name);
				if lb2.get_count() > 0 {
					lb2.set_selection(lb2.get_count() - 1, true);
				}
			}
		}
	});

	let dlg3 = dialog;
	let cfg3 = Arc::clone(&config_manager);
	let lb3 = list_box;
	remove_btn.on_click(move |_| {
		if let Some(sel) = lb3.get_selection()
			&& show_custom_message(&dlg3, "Are you sure you want to remove this command?", "Confirm") == RET_OK
		{
			let mut cfg = cfg3.lock().expect("Config manager lock failed");
			let mut cmds = cfg.get_commands().to_vec();
			if (sel as usize) < cmds.len() {
				cmds.remove(sel as usize);
				cfg.set_commands(&cmds);
				lb3.delete(sel);
			}
		}
	});

	let dlg4 = dialog;
	let cfg4 = Arc::clone(&config_manager);
	ok_btn.on_click(move |_| {
		cfg4.lock().expect("Config manager lock failed").flush();
		dlg4.end_modal(RET_OK);
	});
	let dlg5 = dialog;
	cancel_btn.on_click(move |_| dlg5.end_modal(RET_CANCEL));

	dialog.set_sizer(main_sizer, true);
	dialog.centre();
	let _ = dialog.show_modal();
}
