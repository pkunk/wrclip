extern crate anyhow;
extern crate tempfile;
extern crate wayland_client;

use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::os::unix::io::FromRawFd;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;
use std::{io, thread};

use os_pipe::{PipeReader, PipeWriter};
use std::cell::Cell;
use wayland_client::protocol::{
    wl_compositor, wl_data_device_manager, wl_data_offer::WlDataOffer, wl_seat, wl_shell, wl_shm,
    wl_surface::WlSurface,
};
use wayland_client::{Display, GlobalManager};

pub fn copy(mimes: Vec<String>) -> anyhow::Result<()> {
    let (display, mut event_queue) = Display::connect_to_env()?;
    let globals = GlobalManager::new(&display);

    // roundtrip to retrieve the globals list
    event_queue.sync_roundtrip()?;

    let data_device_manager = globals
        .instantiate_exact::<wl_data_device_manager::WlDataDeviceManager, _>(3, |ddm| {
            ddm.implement_dummy()
        })?;

    let seat = globals.instantiate_exact::<wl_seat::WlSeat, _>(5, |seat| seat.implement_dummy())?;

    let data_device = data_device_manager
        .get_data_device(&seat, |data_device| data_device.implement_dummy())
        .unwrap();

    let mut tmp = tempfile::tempfile()?;
    io::copy(&mut io::stdin(), &mut tmp)?;

    let data_source = data_device_manager
        .create_data_source(move |data_source| {
            data_source.implement_closure(
                move |event, _data_source| {
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
                },
                (),
            )
        })
        .unwrap();

    let _surface = create_wl_surface(&globals)?;

    for mime in mimes {
        data_source.offer(mime.clone());
    }

    data_device.set_selection(Some(&data_source), 0);

    while event_queue.sync_roundtrip().is_ok() {
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

pub fn paste(mimes: Vec<String>) -> anyhow::Result<()> {
    let (display, mut event_queue) = Display::connect_to_env()?;
    let globals = GlobalManager::new(&display);

    // roundtrip to retrieve the globals list
    event_queue.sync_roundtrip()?;

    //    for (id, interface, version) in globals.list() {
    //        println!("{}: {} (version {})", id, interface, version);
    //    }

    let data_device_manager = globals
        .instantiate_exact::<wl_data_device_manager::WlDataDeviceManager, _>(3, |ddm| {
            ddm.implement_dummy()
        })?;

    let seat = globals.instantiate_exact::<wl_seat::WlSeat, _>(5, |seat| seat.implement_dummy())?;

    let (tx, rx) = mpsc::channel();

    let best_index = Rc::new(Cell::new(Option::None));

    let mimes = Rc::new(mimes);
    let _data_device = data_device_manager
        .get_data_device(&seat, |data_device| {
            data_device.implement_closure(
                move |event, _data_device| {
                    use wayland_client::protocol::wl_data_device::Event;
                    let mimes = Rc::clone(&mimes);
                    let best_index = Rc::clone(&best_index);
                    match event {
                        Event::DataOffer { id } => {
                            id.implement_closure(
                                move |event, _data_offer| {
                                    use wayland_client::protocol::wl_data_offer::Event;
                                    #[allow(clippy::single_match)]
                                    match event {
                                        Event::Offer { mime_type } => {
                                            if let Some(n) =
                                                mimes.iter().position(|mime| *mime == mime_type)
                                            {
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
                                },
                                (),
                            );
                        }
                        Event::Selection { id } => {
                            if let Some(offer) = id {
                                let best_index = best_index.get();
                                if let Some(best_index) = best_index {
                                    let mime = mimes[best_index].clone();
                                    let tx = tx.clone();
                                    thread::spawn(move || {
                                        let (reader, writer) = os_pipe::pipe().unwrap();
                                        let _ = do_receive(&offer, mime, reader, writer);
                                        tx.send(()).unwrap();
                                    });
                                } else {
                                    tx.send(()).unwrap();
                                }
                            }
                        }
                        _ => (),
                    }
                },
                (),
            )
        })
        .unwrap();

    let _surface = create_wl_surface(&globals)?;

    while if event_queue.sync_roundtrip().is_ok() {
        let wait: bool = match rx.try_recv() {
            Ok(_) => false,
            Err(TryRecvError::Empty) => true,
            Err(TryRecvError::Disconnected) => false,
        };
        wait
    } else {
        false
    } {
        thread::sleep(Duration::from_millis(10));
    }
    Ok(())
}

fn do_receive(
    offer: &WlDataOffer,
    mime: String,
    mut reader: PipeReader,
    writer: PipeWriter,
) -> io::Result<u64> {
    offer.receive(mime, writer.as_raw_fd());
    drop(writer);
    io::copy(&mut reader, &mut io::stdout())
}

fn create_wl_surface(globals: &GlobalManager) -> anyhow::Result<WlSurface> {
    let buf_x: u32 = 1;
    let buf_y: u32 = 1;
    let mut tmp = tempfile::tempfile()?;
    for _ in 0..(buf_x * buf_y * 4) {
        let _ = tmp.write_all(&[0x00]);
    }
    let _ = tmp.flush();

    let compositor = globals
        .instantiate_exact::<wl_compositor::WlCompositor, _>(1, |comp| comp.implement_dummy())?;
    let surface = compositor
        .create_surface(|surface| surface.implement_dummy())
        .unwrap();

    let shm = globals.instantiate_exact::<wl_shm::WlShm, _>(1, |shm| shm.implement_dummy())?;
    let pool = shm
        .create_pool(
            tmp.as_raw_fd(),
            (buf_x * buf_y * 4) as i32, // size in bytes of the shared memory (4 bytes per pixel)
            |pool| pool.implement_dummy(),
        )
        .unwrap();
    let buffer = pool
        .create_buffer(
            0,                        // Start of the buffer in the pool
            buf_x as i32,             // width of the buffer in pixels
            buf_y as i32,             // height of the buffer in pixels
            (buf_x * 4) as i32, // number of bytes between the beginning of two consecutive lines
            wl_shm::Format::Argb8888, // chosen encoding for the data
            |buffer| buffer.implement_dummy(),
        )
        .unwrap();

    let shell =
        globals.instantiate_exact::<wl_shell::WlShell, _>(1, |shell| shell.implement_dummy())?;
    let shell_surface = shell
        .get_shell_surface(&surface, |shellsurface| {
            shellsurface.implement_closure(
                |event, shell_surface| {
                    use wayland_client::protocol::wl_shell_surface::Event;
                    // This ping/pong mechanism is used by the wayland server to detect
                    // unresponsive applications
                    if let Event::Ping { serial } = event {
                        shell_surface.pong(serial);
                    }
                },
                (),
            )
        })
        .unwrap();

    shell_surface.set_toplevel();
    surface.attach(Some(&buffer), 0, 0);
    surface.commit();
    Ok(surface)
}
