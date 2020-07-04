use std::any::Any;
use std::cell::Cell;
use std::error::Error;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::os::unix::io::FromRawFd;
use std::rc::Rc;
use std::time::Duration;
use std::{io, thread};

use crossbeam_channel::select;
use os_pipe::{PipeReader, PipeWriter};
use wayland_client::protocol::{
    wl_compositor, wl_data_device_manager, wl_data_offer, wl_seat, wl_shm,
};
use wayland_client::{Display, GlobalManager};
use wayland_protocols::xdg_shell::client::xdg_wm_base;

pub fn copy(mimes: Vec<String>) -> Result<(), Box<dyn Error>> {
    let display = Display::connect_to_env()?;
    let mut event_queue = display.create_event_queue();
    let display = (*display).clone().attach(event_queue.token());
    let globals = GlobalManager::new(&display);

    // roundtrip to retrieve the globals list
    event_queue.sync_roundtrip(&mut (), |_, _, _| {})?;

    let data_device_manager =
        globals.instantiate_exact::<wl_data_device_manager::WlDataDeviceManager>(3)?;

    let seat = globals.instantiate_exact::<wl_seat::WlSeat>(5)?;

    let data_device = data_device_manager.get_data_device(&seat);

    let mut tmp = tempfile::tempfile()?;
    io::copy(&mut io::stdin(), &mut tmp)?;

    let data_source = data_device_manager.create_data_source();
    data_source.quick_assign(move |_data_source, event, _| {
        use wayland_client::protocol::wl_data_source::Event;
        #[allow(unused_variables)]
        #[allow(clippy::single_match)]
        match event {
            Event::Send { mime_type, fd } => {
                let mut f = unsafe { File::from_raw_fd(fd) };
                tmp.seek(SeekFrom::Start(0)).unwrap();
                let _ = io::copy(&mut tmp, &mut f);
            }
            _ => (),
        }
    });

    let _surface = create_xdg_surface(&globals)?;

    event_queue.sync_roundtrip(&mut (), |_, _, _| {})?;

    // thread::sleep(Duration::from_millis(50));

    for mime in mimes {
        data_source.offer(mime.clone());
    }

    event_queue.sync_roundtrip(&mut (), |_, _, _| {})?;

    data_device.set_selection(Some(&data_source), 0);

    event_queue.sync_roundtrip(&mut (), |_, _, _| {})?;

    Ok(())
}

pub fn paste(mimes: Vec<String>) -> Result<(), Box<dyn Error>> {
    let display = Display::connect_to_env()?;
    let mut event_queue = display.create_event_queue();
    let display = (*display).clone().attach(event_queue.token());
    let globals = GlobalManager::new(&display);

    event_queue.sync_roundtrip(&mut (), |_, _, _| {})?;

    let data_device_manager =
        globals.instantiate_exact::<wl_data_device_manager::WlDataDeviceManager>(3)?;

    let seat = globals.instantiate_exact::<wl_seat::WlSeat>(5)?;

    let (sender, receiver) = crossbeam_channel::bounded::<()>(0);

    let best_index = Rc::new(Cell::new(Option::None));

    let mimes = Rc::new(mimes);
    let data_device = data_device_manager.get_data_device(&seat);
    data_device.quick_assign(move |_data_device, event, _| {
        use wayland_client::protocol::wl_data_device::Event;
        let mimes = Rc::clone(&mimes);
        let best_index = Rc::clone(&best_index);
        match event {
            Event::DataOffer { id } => id.quick_assign(move |_data_offer, event, _| {
                use wayland_client::protocol::wl_data_offer::Event;
                #[allow(clippy::single_match)]
                match event {
                    Event::Offer { mime_type } => {
                        if let Some(n) = mimes.iter().position(|mime| *mime == mime_type) {
                            let current_best = best_index.get();
                            if let Some(k) = current_best {
                                if n < k {
                                    best_index.set(Some(n));
                                }
                            } else {
                                best_index.set(Some(n));
                            }
                        }
                    }
                    _ => (),
                }
            }),
            Event::Selection { id } => {
                if let Some(offer) = id {
                    let best_index = best_index.get();
                    if let Some(best_index) = best_index {
                        let mime = mimes[best_index].clone();
                        let sender_inner = sender.clone();
                        thread::spawn(move || {
                            let (reader, writer) = os_pipe::pipe().unwrap();
                            let _ = do_receive(&offer, mime, reader, writer);
                            sender_inner.send(()).unwrap();
                        });
                    } else {
                        sender.send(()).unwrap();
                    }
                }
            }
            _ => (),
        }
    });

    let _surface = create_xdg_surface(&globals)?;

    while event_queue.sync_roundtrip(&mut (), |_, _, _| {}).is_ok() {
        select! {
            recv(receiver) -> _ => break,
            default(Duration::from_millis(10)) => continue,
        }
    }

    Ok(())
}

fn do_receive(
    offer: &wl_data_offer::WlDataOffer,
    mime: String,
    mut reader: PipeReader,
    writer: PipeWriter,
) -> io::Result<u64> {
    offer.receive(mime, writer.as_raw_fd());
    drop(writer);
    io::copy(&mut reader, &mut io::stdout())
}

fn create_xdg_surface(globals: &GlobalManager) -> Result<Box<dyn Any>, Box<dyn Error>> {
    let buf_x: u32 = 1;
    let buf_y: u32 = 1;
    let mut tmp = tempfile::tempfile()?;
    for _ in 0..(buf_x * buf_y * 4) {
        tmp.write_all(&[0x00])?;
    }
    tmp.flush()?;

    let compositor = globals.instantiate_exact::<wl_compositor::WlCompositor>(1)?;
    let surface = compositor.create_surface();

    let shm = globals.instantiate_exact::<wl_shm::WlShm>(1)?;
    let pool = shm.create_pool(
        tmp.as_raw_fd(),
        (buf_x * buf_y * 4) as i32, // size in bytes of the shared memory (4 bytes per pixel)
    );
    let buffer = pool.create_buffer(
        0,                        // Start of the buffer in the pool
        buf_x as i32,             // width of the buffer in pixels
        buf_y as i32,             // height of the buffer in pixels
        (buf_x * 4) as i32,       // number of bytes between the beginning of two consecutive lines
        wl_shm::Format::Argb8888, // chosen encoding for the data
    );

    let shell = globals.instantiate_exact::<xdg_wm_base::XdgWmBase>(3)?;
    shell.quick_assign(|shell, event, _| {
        use xdg_wm_base::Event;
        if let Event::Ping { serial } = event {
            shell.pong(serial);
        }
    });

    let shell_surface = shell.get_xdg_surface(&surface);
    let shell_surface = shell_surface.get_toplevel();

    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    Ok(Box::new(shell_surface))
}
