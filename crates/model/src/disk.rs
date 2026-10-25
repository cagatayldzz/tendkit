use crate::{AppModel, DiskReport};
use gpui_kit::*;
use std::{
  path::PathBuf,
  sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
  },
};
use tendkit_storage::disk::{index_disk, DiskIndex};

#[derive(Default)]
pub struct DiskState {
  pub path: PathBuf,
  pub report: Option<DiskReport>,
  pub scanning: bool,
  pub error: Option<String>,
  pub cache_error: bool,
  pub index: Option<DiskIndex>,
  cancel: Arc<AtomicBool>,
}

impl AppModel {
  /// Open an indexed folder without touching the filesystem or starting a scan.
  pub fn browse_disk(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    self.disk.report = self.disk.index.as_ref().and_then(|index| index.report(&path)).cloned();
    self.disk.path = path;
    self.disk.error = None;
    cx.notify();
  }

  /// On first entry, restore the saved index or build one for the disk root.
  pub fn scan_disk(&mut self, path: PathBuf, cx: &mut Context<Self>) {
    if self.disk.index.is_some() {
      self.browse_disk(path, cx);
      return;
    }
    self.disk.path = path;
    self.start_disk_index(false, cx);
  }

  pub fn refresh_disk_index(&mut self, cx: &mut Context<Self>) {
    self.start_disk_index(true, cx);
  }

  fn start_disk_index(&mut self, force: bool, cx: &mut Context<Self>) {
    if self.disk.scanning {
      return;
    }
    self.disk.scanning = true;
    self.disk.error = None;
    self.disk.cache_error = false;
    self.disk.cancel = Arc::new(AtomicBool::new(false));
    let cancel = self.disk.cancel.clone();
    let task = cx.background_executor().spawn(async move {
      let root = tendkit_storage::disk::disk_root();
      let path = DiskIndex::cache_path();
      if !force {
        if let Some(index) = DiskIndex::load(&path, &root) {
          return Ok((index, false));
        }
      }
      let index = index_disk(&root, &cancel)?;
      if cancel.load(Ordering::Relaxed) {
        if let Some(previous) = DiskIndex::load(&path, &root) {
          return Ok((previous, false));
        }
      }
      let cache_error = index.save(&path).is_err();
      Ok::<_, String>((index, cache_error))
    });
    cx.spawn(async move |this, cx| {
      let result = task.await;
      let _ = this.update(cx, |model, cx| {
        model.disk.scanning = false;
        match result {
          Ok((index, cache_error)) => {
            model.disk.cache_error = cache_error;
            model.disk.report = index.report(&model.disk.path).cloned();
            model.disk.index = Some(index);
          }
          Err(error) => model.disk.error = Some(error),
        }
        cx.notify();
      });
    })
    .detach();
    cx.notify();
  }

  pub fn cancel_disk(&self) {
    self.disk.cancel.store(true, Ordering::Relaxed);
  }
}
