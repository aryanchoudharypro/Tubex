use crate::video_info::VideoInfo;

#[derive(Debug)]
pub enum AppEvent {
	Status(String, String),
	Output(String, String),
	Finished(String),
	Error(String, String),
	ShowOptions(String, Vec<VideoInfo>),
	ShowOptionsForMultipleUrls(Vec<String>, Box<VideoInfo>),
	StartupCheck,
	StartupResult(bool, bool),
	DownloadComplete(Result<(), String>),
	DownloadProgress(String, i32),
	RequestFetch(String),
}
