//! 图标缓存与对外接口。

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};

use super::platform::extract_icon_rgba;

/// LRU 容量上限。
const MAX_ICON_CACHE: usize = 128;

/// 解码后的图标像素（RGBA，非预乘）。
#[derive(Clone)]
pub struct IconImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// 全局 LRU 缓存：应用原始路径 → RGBA 像素。
static CACHE: LazyLock<Mutex<LruCache>> =
    LazyLock::new(|| Mutex::new(LruCache::new(MAX_ICON_CACHE)));

/// 串行提取锁：保证同一时刻只有一个原生提取任务在执行。
static EXTRACT_LOCK: Mutex<()> = Mutex::new(());

/// 简单的 LRU 缓存。利用 VecDeque 维护访问顺序，HashMap 存放数据。
struct LruCache {
    map: HashMap<String, IconImage>,
    order: VecDeque<String>,
    cap: usize,
}

impl LruCache {
    fn new(cap: usize) -> Self {
        Self {
            map: HashMap::new(),
            order: VecDeque::new(),
            cap,
        }
    }

    /// 命中时刷新顺序后返回克隆（像素体积不大，克隆可接受，避免持有锁跨提取）。
    fn get(&mut self, key: &str) -> Option<IconImage> {
        if let Some(val) = self.map.get(key) {
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.to_string());
            Some(val.clone())
        } else {
            None
        }
    }

    fn put(&mut self, key: String, val: IconImage) {
        if self.map.contains_key(&key) {
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.clone());
            self.map.insert(key, val);
            return;
        }
        if self.map.len() >= self.cap {
            if let Some(old) = self.order.pop_front() {
                self.map.remove(&old);
            }
        }
        self.map.insert(key.clone(), val);
        self.order.push_back(key);
    }
}

/// 对外入口：根据应用路径返回 RGBA 像素（带缓存）。
pub fn get_icon_rgba(path: &str) -> Option<IconImage> {
    // 1. 快速路径：命中缓存直接返回
    if let Some(b) = CACHE.lock().ok().and_then(|mut c| c.get(path)) {
        return Some(b);
    }

    // 2. 串行提取
    let _guard = EXTRACT_LOCK.lock().ok()?;

    // 拿到锁后再次检查缓存（防止并发重复提取）
    if let Some(b) = CACHE.lock().ok().and_then(|mut c| c.get(path)) {
        return Some(b);
    }

    let pixels = extract_icon_rgba(path)?;

    if let Ok(mut c) = CACHE.lock() {
        c.put(path.to_string(), pixels.clone());
    }
    Some(pixels)
}

/// 对外入口（PNG 版）：仅供测试校验 PNG 魔数使用。
#[cfg(test)]
pub fn get_icon(path: &str) -> Option<Vec<u8>> {
    let img = get_icon_rgba(path)?;
    encode_png(img.width, img.height, &img.rgba)
}

/// RGBA → PNG 编码（平台无关，仅测试用）。
#[cfg(test)]
fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}
