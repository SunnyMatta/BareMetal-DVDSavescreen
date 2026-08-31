#![no_std]
#![no_main]

extern crate alloc;
use alloc::{boxed::Box, vec::Vec};
use uefi::{prelude::*, print};
use embedded_graphics::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use embedded_graphics::pixelcolor::Rgb888;
use core::{convert::Infallible, panic::PanicInfo};
use tinybmp::Bmp;

#[allow(dead_code)]
pub struct UEFIDisplay {
    fb_ptr: *mut u32,
    fb_stride: usize, 
    fb_resolution: (usize, usize),
    is_rgb: bool,     
    backbuffer : Vec<u32>
}

impl UEFIDisplay {
    pub fn flush(&mut self, fb_ptr: *mut u32){
        let total_pixels = self.fb_resolution.0 * self.fb_resolution.1;
        if self.backbuffer.len() == total_pixels{
            unsafe { core::ptr::copy_nonoverlapping(
                self.backbuffer.as_ptr(),
                fb_ptr, 
                total_pixels
            ) };
        }
    }
}

impl DrawTarget for UEFIDisplay {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> core::result::Result<(), Infallible>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>
    {
        for pixel in pixels {
            let Pixel(coord, color) = pixel;
            if coord.x < 0 || coord.y < 0 {
                continue;
            }
            let x = coord.x as usize;
            let y = coord.y as usize;

            if x >= self.fb_resolution.0 || y >= self.fb_resolution.1 {
                continue;
            }

            let offset = (y * self.fb_stride) + x;

            let raw_color: u32 = if self.is_rgb {
                ((color.r() as u32) << 16) | ((color.g() as u32) << 8) | (color.b() as u32)
            } else {
                ((color.b() as u32) << 16) | ((color.g() as u32) << 8) | (color.r() as u32)
            };


            if offset < self.backbuffer.len(){
                self.backbuffer[offset] = raw_color;
            }
            //unsafe {
            //    // Write the full 4-byte pixel safely
            //    *self.fb_ptr.add(offset) = raw_color;
            //}
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &embedded_graphics::primitives::Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let intersection = area.intersection(&self.bounding_box());
        if intersection.is_zero_sized(){
            return Ok(());
        }
        let x_start = intersection.top_left.x as usize;
        let y_start = intersection.top_left.y as usize;
        let width = intersection.size.width as usize;
        let height = intersection.size.height as usize;

        let mut color_iter = colors.into_iter();

        for y in 0..height{
            let screen_y = y_start + y;
            let row_offset = (screen_y * self.fb_stride) + x_start;

            for x in 0..width{
                if let Some(color) = color_iter.next() {
                let raw_color: u32 = if self.is_rgb {
                    ((color.r() as u32) << 16) | ((color.g() as u32) << 8) | (color.b() as u32)
                } else {
                    ((color.b() as u32) << 16) | ((color.g() as u32) << 8) | (color.r() as u32)
                };

                let target_idx = row_offset + x;
                if target_idx < self.backbuffer.len(){
                    self.backbuffer[target_idx] = raw_color;
                }
                //unsafe {
                //    *self.fb_ptr.add(row_offset + x) = raw_color;
                //}
               
            }
            }
        }

        
        Ok(())
    }
}

impl OriginDimensions for UEFIDisplay {
    fn size(&self) -> Size {
        Size::new(self.fb_resolution.0 as u32, self.fb_resolution.1 as u32)
    }
}


#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    
    let _ = uefi::system::with_stdout(|stdout| {
        stdout.clear()
    });

    let gop_handle = uefi::boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
    let mut gop = uefi::boot::open_protocol_exclusive::<GraphicsOutput>(gop_handle).unwrap();

    let mode_info = gop.current_mode_info();
    let resolution = mode_info.resolution();
    let backbuffer_len = resolution.0 * resolution.1;

    let is_rgb = match mode_info.pixel_format() {
        PixelFormat::Rgb => true,
        _ => false,
    };

    let backbuffer: Box<[u32]> = alloc::vec![0; backbuffer_len].into_boxed_slice();

    let mut display = UEFIDisplay {
        fb_ptr: gop.frame_buffer().as_mut_ptr() as *mut u32,
        fb_stride: mode_info.stride(), // Raw pixel count per row
        fb_resolution: (resolution.0, resolution.1),
        is_rgb,
        backbuffer: backbuffer.to_vec(),
    };


    // LOGIC ////////////////////////////////////////////////

    let rawdata = include_bytes!("../data/DVD_logo.svg.bmp");
    let data : Bmp<Rgb888> = Bmp::from_slice(rawdata).unwrap();

    let header = data.size();

    let mut position_x: i32 = 100;
    let mut position_y: i32 = 100;

    let xstart = (0 + header.width as i32) - display.fb_resolution.0 as i32;
    let ystart = (0 + header.height as i32) - display.fb_resolution.1 as i32;
    
    let mut move_xreverse = false;
    let mut move_yreverse = false;
    // Infinite loop so the screen stays visible in OVMF/QEMU
    loop {
        let starttm = unsafe{core::arch::x86_64::_rdtsc()};
        let fb_ptr = gop.frame_buffer().as_mut_ptr() as *mut u32;

        let xcollider = (position_x + header.width as i32) - display.fb_resolution.0 as i32;
        let ycollider = (position_y + header.height as i32) - display.fb_resolution.1 as i32;

        display.backbuffer.fill(0);
        

        embedded_graphics::image::Image::new(&data, Point::new(position_x, position_y))
            .draw(&mut display)
            .unwrap();

        display.flush(fb_ptr);

        if move_xreverse == false{
            position_x += 1;
        }else if move_xreverse == true {
            position_x -= 1
        }
        
        if move_yreverse == false{
            position_y += 1;
        }else if move_yreverse == true {
            position_y -= 1
        }

        if xcollider >= 0{
            move_xreverse = true
        }else if xcollider == xstart {
            move_xreverse = false
        }

        if ycollider >= 0{
            move_yreverse = true
        }else if ycollider == ystart {
            move_yreverse = false
        }
        

        //clear the previous image by calling clear or by drawing a background color over the previous image
        let stoptm = unsafe{core::arch::x86_64::_rdtsc()};
        let elapsed = stoptm - starttm;
        print!("{}", elapsed);
        core::hint::spin_loop();
    }

    //////////////////////////////////////////////////////////
}


#[global_allocator]
static ALLOCATOR: uefi::allocator::Allocator = uefi::allocator::Allocator;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
