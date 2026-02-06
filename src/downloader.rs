use std::sync::Arc;
use parking_lot::RwLock;
use std::path::PathBuf;
use anyhow::Result;
use futures::StreamExt;
use std::fs::File;
use std::io::Write;

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub current: u64,
    pub total: u64,
    pub speed: f64, // MB/s
}

pub struct Downloader {
    progress: Arc<RwLock<DownloadProgress>>,
    _threads: u32,
}

impl Downloader {
    pub fn new(threads: u32) -> Self {
        Self {
            progress: Arc::new(RwLock::new(DownloadProgress {
                current: 0,
                total: 0,
                speed: 0.0,
            })),
            _threads: threads,
        }
    }
    
    pub async fn download(&self, url: &str, path: PathBuf) -> Result<()> {
        let client = reqwest::Client::new();
        let response = client.get(url).send().await?;
        
        let total_size = response
            .content_length()
            .ok_or_else(|| anyhow::anyhow!("无法获取文件大小"))?;
        
        {
            let mut progress = self.progress.write();
            progress.total = total_size;
            progress.current = 0;
        }
        
        let mut file = File::create(&path)?;
        let mut stream = response.bytes_stream();
        let mut downloaded = 0u64;
        let start_time = std::time::Instant::now();
        
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            file.write_all(&chunk)?;
            
            downloaded += chunk.len() as u64;
            
            let elapsed = start_time.elapsed().as_secs_f64();
            let speed = if elapsed > 0.0 {
                (downloaded as f64 / elapsed) / (1024.0 * 1024.0)
            } else {
                0.0
            };
            
            {
                let mut progress = self.progress.write();
                progress.current = downloaded;
                progress.speed = speed;
            }
        }
        
        Ok(())
    }
    
    #[allow(dead_code)]
    pub fn get_progress(&self) -> DownloadProgress {
        self.progress.read().clone()
    }
    
    #[allow(dead_code)]
    pub async fn download_plugin(&self, url: &str, drive_letter: &str, filename: &str) -> Result<()> {
        let download_path = format!("{}\\ce-apps", drive_letter);
        std::fs::create_dir_all(&download_path)?;
        
        let file_path = PathBuf::from(download_path).join(filename);
        self.download(url, file_path).await
    }
}