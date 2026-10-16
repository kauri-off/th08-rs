use anyhow::Result;

#[derive(Clone)]
pub enum Source {
    #[cfg(not(target_arch = "wasm32"))]
    Native(std::path::PathBuf),
    #[cfg(target_arch = "wasm32")]
    Web(std::rc::Rc<Vec<web_sys::File>>),
}

impl Source {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn native(dir: std::path::PathBuf) -> Self {
        Source::Native(dir)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn web(files: Vec<web_sys::File>) -> Self {
        Source::Web(std::rc::Rc::new(files))
    }

    pub async fn read(&self, name: &str) -> Result<Vec<u8>> {
        self.fetch(name, None).await
    }

    pub async fn read_range(&self, name: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        self.fetch(name, Some((offset, len))).await
    }

    #[cfg(not(target_arch = "wasm32"))]
    async fn fetch(&self, name: &str, range: Option<(u64, usize)>) -> Result<Vec<u8>> {
        use std::io::{Read, Seek, SeekFrom};
        let Source::Native(dir) = self;
        let path = dir.join(name);
        Ok(match range {
            None => std::fs::read(path)?,
            Some((off, len)) => {
                let mut f = std::fs::File::open(path)?;
                f.seek(SeekFrom::Start(off))?;
                let mut buf = vec![0; len];
                f.read_exact(&mut buf)?;
                buf
            }
        })
    }

    #[cfg(target_arch = "wasm32")]
    async fn fetch(&self, name: &str, range: Option<(u64, usize)>) -> Result<Vec<u8>> {
        use wasm_bindgen_futures::JsFuture;
        let Source::Web(files) = self;
        let err = |e: wasm_bindgen::JsValue| anyhow::anyhow!("{e:?}");
        let file = files
            .iter()
            .find(|f| f.name().eq_ignore_ascii_case(name))
            .ok_or_else(|| anyhow::anyhow!("{name} was not provided"))?;
        let blob = match range {
            None => file.slice().map_err(err)?,
            Some((off, len)) => file.slice_with_f64_and_f64(off as f64, (off + len as u64) as f64).map_err(err)?,
        };
        let buf = JsFuture::from(blob.array_buffer()).await.map_err(err)?;
        Ok(js_sys::Uint8Array::new(&buf).to_vec())
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn(fut: impl std::future::Future<Output = ()> + Send + 'static) {
    std::thread::spawn(move || pollster::block_on(fut));
}

#[cfg(target_arch = "wasm32")]
pub fn spawn(fut: impl std::future::Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(fut);
}
