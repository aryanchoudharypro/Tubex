use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Format {
	pub format_id: String,
	pub format_note: Option<String>,
	pub ext: Option<String>,
	pub vcodec: Option<String>,
	pub acodec: Option<String>,
	pub language: Option<String>,
	pub width: Option<u32>,
	pub height: Option<u32>,
	pub filesize: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct VideoInfo {
	pub id: String,
	pub title: String,
	pub uploader: Option<String>,
	pub channel_url: Option<String>,
	pub duration: Option<f64>,
	pub webpage_url: Option<String>,
	pub url: Option<String>,
	pub view_count: Option<u64>,
	pub playlist_count: Option<u32>,
	#[serde(rename = "_type")]
	pub result_type: Option<String>,
	#[serde(default)]
	pub formats: Vec<Format>,
}

impl VideoInfo {
	pub fn get_video_formats(&self) -> Vec<Format> {
		self.formats.iter().filter(|f| f.vcodec.as_deref().is_some_and(|v| v != "none")).cloned().collect()
	}

	pub fn get_audio_formats(&self) -> Vec<Format> {
		self.formats
			.iter()
			.filter(|f| {
				f.acodec.as_deref().is_some_and(|a| a != "none") && f.vcodec.as_deref().is_none_or(|v| v == "none")
			})
			.cloned()
			.collect()
	}
}
