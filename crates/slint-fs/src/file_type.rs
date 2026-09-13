

pub enum PlatformFileFormat {
    Jpg,
    Jpeg,
    Png,
    Mp3,
    Wav,
    Mp4,
}

impl PlatformFileFormat {

    pub fn file_type(&self) -> PlatformFileType {
        match self {
            PlatformFileFormat::Jpg => PlatformFileType::Image,
            PlatformFileFormat::Jpeg => PlatformFileType::Image,
            PlatformFileFormat::Png => PlatformFileType::Image,
            PlatformFileFormat::Mp3 => PlatformFileType::Audio,
            PlatformFileFormat::Wav => PlatformFileType::Audio,
            PlatformFileFormat::Mp4 => PlatformFileType::Video,
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            PlatformFileFormat::Jpg => "image/jpg",
            PlatformFileFormat::Jpeg => "image/jpeg",
            PlatformFileFormat::Png => "image/png",
            PlatformFileFormat::Wav => "audio/wav",
            PlatformFileFormat::Mp3 => "audio/mp3",
            PlatformFileFormat::Mp4 => "video/mp4",
        }
    }
}


pub enum PlatformFileType {
    Image,
    Video,
    Audio,
}


impl PlatformFileType {
    pub fn mime_type(&self) -> &'static str {
        match self {
            PlatformFileType::Image => "image/*",
            PlatformFileType::Video => "video/*",
            PlatformFileType::Audio => "audio/*",
        }
    }
}

