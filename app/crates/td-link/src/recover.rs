use crate::bundle::Bundle;
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
    total: usize,
}
impl ProgressCallbacks for Progress<'_> {
    fn init(&mut self, _addr: u32, total: usize) {
        self.total = total;
    }
    fn update(&mut self, current: usize) {
        (self.emit)(UpdateState::Recovering {
            pct: ((current * 100) / self.total.max(1)).min(100) as u8,
        });
    }
    fn verifying(&mut self) {}
    fn finish(&mut self, _skipped: bool) {}
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
            .write_bins_to_flash(&segments, &mut Progress { emit, total: 1 })
            .map_err(|e| e.to_string())?;
        flasher
            .connection()
            .reset_after(true, Chip::Esp32s3)
            .map_err(|e| e.to_string())
    }
}
