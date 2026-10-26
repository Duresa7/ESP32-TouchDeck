use crate::bundle::{Bundle, Image};
use crate::update::UpdateState;
use espflash::connection::{Connection, ResetAfterOperation, ResetBeforeOperation};
use espflash::flasher::Flasher;
use espflash::image_format::Segment;
use espflash::target::{Chip, ProgressCallbacks};

pub trait Recovery {
    fn flash(
        &mut self,
        bundle: &Bundle,
        progress: &mut dyn FnMut(UpdateState),
    ) -> Result<(), String>;
}
pub struct RomRecovery<'a>(pub &'a str);
struct Progress<'a> {
    emit: &'a mut dyn FnMut(UpdateState),
    images: &'a [Image],
    image: usize,
    completed: usize,
    total: usize,
    chunks: usize,
}
impl<'a> Progress<'a> {
    fn new(emit: &'a mut dyn FnMut(UpdateState), images: &'a [Image]) -> Self {
        Self {
            emit,
            images,
            image: 0,
            completed: 0,
            total: images.iter().map(|image| image.data.len()).sum(),
            chunks: 1,
        }
    }
}
impl ProgressCallbacks for Progress<'_> {
    fn init(&mut self, _addr: u32, total: usize) {
        self.chunks = total;
    }
    fn update(&mut self, current: usize) {
        let bytes =
            self.completed + self.images[self.image].data.len() * current / self.chunks.max(1);
        (self.emit)(UpdateState::Recovering {
            pct: (bytes * 100 / self.total.max(1)).min(100) as u8,
        });
    }
    fn verifying(&mut self) {}
    fn finish(&mut self, _skipped: bool) {
        self.update(self.chunks);
        self.completed += self.images[self.image].data.len();
        self.image += 1;
    }
}
impl Recovery for RomRecovery<'_> {
    fn flash(&mut self, bundle: &Bundle, emit: &mut dyn FnMut(UpdateState)) -> Result<(), String> {
        let info = serialport::available_ports()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|p| p.port_name == self.0)
            .ok_or("recovery port not found")?;
        let serialport::SerialPortType::UsbPort(usb) = info.port_type else {
            return Err("recovery requires USB-Serial/JTAG".into());
        };
        if usb.vid != crate::VID || usb.pid != crate::PID {
            return Err("recovery requires Espressif USB-Serial/JTAG".into());
        }
        let port = serialport::new(self.0, 115200)
            .timeout(std::time::Duration::from_secs(3))
            .dtr_on_open(false)
            .open_native()
            .map_err(|e| e.to_string())?;
        let connection = Connection::new(
            port,
            usb,
            ResetAfterOperation::HardReset,
            ResetBeforeOperation::UsbReset,
            115200,
        );
        let mut flasher =
            Flasher::connect(connection, true, true, false, Some(Chip::Esp32s3), None)
                .map_err(|e| e.to_string())?;
        flasher
            .erase_region(0xf000, 0x2000)
            .map_err(|e| e.to_string())?;
        let segments: Vec<_> = bundle
            .images
            .iter()
            .map(|i| Segment::new(i.offset, &i.data))
            .collect();
        flasher
            .write_bins_to_flash(&segments, &mut Progress::new(emit, &bundle.images))
            .map_err(|e| e.to_string())?;
        flasher
            .connection()
            .reset_after(true, Chip::Esp32s3)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_progress_never_restarts_between_images() {
        let mut percentages = Vec::new();
        let mut emit = |state| {
            if let UpdateState::Recovering { pct } = state {
                percentages.push(pct);
            }
        };
        let bundle =
            Bundle::parse(include_bytes!("../tests/fixtures/touchdeck-test.tdfw")).unwrap();
        let mut progress = Progress::new(&mut emit, &bundle.images);
        progress.init(0, 2);
        progress.update(1);
        progress.update(2);
        progress.finish(false);
        progress.init(0x8000, 1);
        progress.update(1);
        progress.finish(false);
        progress.init(0x40000, 4);
        for chunk in 1..=4 {
            progress.update(chunk);
        }
        progress.finish(false);
        assert!(
            percentages.windows(2).all(|pair| pair[0] <= pair[1]),
            "{percentages:?}"
        );
        assert_eq!(percentages.last(), Some(&100));
        assert!(!percentages[..3].contains(&100));
    }
}
