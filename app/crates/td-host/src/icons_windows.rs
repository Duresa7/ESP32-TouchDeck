//! App icons on Windows, through the shell's `IShellItemImageFactory`, the image Explorer shows.

use std::path::Path;

use image::{Rgba, RgbaImage};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, HBITMAP, HGDIOBJ,
};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
use windows::core::HSTRING;

const REQUEST: i32 = 256;

pub fn app_icon(path: &Path) -> Result<RgbaImage, String> {
    let fail = |error: windows::core::Error| format!("{}: {}", path.display(), error.message());
    // SAFETY: COM is initialised on this thread for the calls below and released after; the
    // bitmap is read and deleted before return.
    unsafe {
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = (|| {
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None)
                    .map_err(fail)?;
            let bitmap = factory
                .GetImage(
                    SIZE {
                        cx: REQUEST,
                        cy: REQUEST,
                    },
                    SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
                )
                .map_err(fail)?;
            let image = read_bitmap(bitmap);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            image.ok_or_else(|| format!("{}: the shell returned no pixels", path.display()))
        })();
        if init.is_ok() {
            CoUninitialize();
        }
        result
    }
}

/// Reads a 32-bit bitmap top-down. The shell hands back premultiplied BGRA, so the colours
/// are divided by alpha; a bitmap with no alpha at all is taken as opaque.
unsafe fn read_bitmap(bitmap: HBITMAP) -> Option<RgbaImage> {
    unsafe {
        let dc = CreateCompatibleDC(None);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        // The first call fills in the size.
        let sized = GetDIBits(dc, bitmap, 0, 0, None, &mut info, DIB_RGB_COLORS);
        let (width, height) = (info.bmiHeader.biWidth, info.bmiHeader.biHeight.abs());
        if sized == 0 || width <= 0 || height <= 0 {
            let _ = DeleteDC(dc);
            return None;
        }
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB.0;
        info.bmiHeader.biHeight = -height;
        let mut bgra = vec![0u8; (width * height * 4) as usize];
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            Some(bgra.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(dc);
        if lines == 0 {
            return None;
        }
        let has_alpha = bgra.chunks_exact(4).any(|px| px[3] != 0);
        let mut image = RgbaImage::new(width as u32, height as u32);
        for (px, out) in bgra.chunks_exact(4).zip(image.pixels_mut()) {
            let [b, g, r, a] = [px[0], px[1], px[2], px[3]];
            *out = if !has_alpha {
                Rgba([r, g, b, 255])
            } else if a == 0 {
                Rgba([0, 0, 0, 0])
            } else {
                let un =
                    |c: u8| ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
                Rgba([un(r), un(g), un(b), a])
            };
        }
        Some(image)
    }
}
