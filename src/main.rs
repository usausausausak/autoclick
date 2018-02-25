extern crate xcb;
extern crate failure;
#[macro_use]
extern crate failure_derive;

use std::{env, time, thread};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Fail)]
enum XError {
    #[fail(display = "query root error")]
    RootError,
    #[fail(display = "connect error: {}", _0)]
    ConnectError(xcb::ConnError),
    #[fail(display = "xcb error: {}", _0)]
    GenericError(xcb::GenericError),
}

impl From<xcb::ConnError> for XError {
    fn from(cause: xcb::ConnError) -> Self {
        XError::ConnectError(cause)
    }
}

impl From<xcb::GenericError> for XError {
    fn from(cause: xcb::GenericError) -> Self {
        XError::GenericError(cause)
    }
}

const CLICK_DELAY: u64 = 300;
const STOP_THRESHOLD: i16 = 30;

fn pointer_click(conn: &xcb::Connection) {
    xcb::test::fake_input(conn, xcb::BUTTON_PRESS, 1,
                          0, xcb::WINDOW_NONE, 0, 0, 0);
    conn.flush();
    xcb::test::fake_input(conn, xcb::BUTTON_RELEASE, 1,
                          0, xcb::WINDOW_NONE, 0, 0, 0);
    conn.flush();
}

fn pointer_position(conn: &xcb::Connection, root: xcb::Window)
    -> Result<(i16, i16), xcb::GenericError>
{

    let cookie = xcb::query_pointer(&conn, root);
    cookie.get_reply().map(|r| (r.root_x(), r.root_y()))
}

fn auto_click(running: &Arc<AtomicBool>) -> Result<u64, XError> {
    let (conn, _) = xcb::Connection::connect(None)?;

    let root = conn.get_setup().roots()
                               .next().ok_or(XError::RootError)?
                               .root();

    let pointer_start = pointer_position(&conn, root)?;

    #[cfg(debug_assertions)]
    println!("pointer at {}, {}", pointer_start.0, pointer_start.1);

    let need_stop = |x: i16, y: i16| -> bool {
        let dx = x - pointer_start.0;
        let dy = y - pointer_start.1;
        dx.abs() >= STOP_THRESHOLD || dy.abs() >= STOP_THRESHOLD
    };

    let mut click_time: u64 = 0;
    while running.load(Ordering::SeqCst) {
        click_time += 1;
        pointer_click(&conn);
        thread::sleep(time::Duration::from_millis(CLICK_DELAY));

        let (x, y) = pointer_position(&conn, root)?;
        if need_stop(x, y) {
            #[cfg(debug_assertions)]
            println!("moved to {}, {}", x, y);
            break;
        }
    }

    Ok(click_time)
}

fn main() {
    let running = Arc::new(AtomicBool::new(true));
    let click_thread = {
        let running = running.clone();
        thread::spawn(move || {
            match auto_click(&running) {
                Ok(click_time) => println!("clicked {} times", click_time),
                Err(err) => eprintln!("{}", err),
            }

            running.store(false, Ordering::SeqCst);
        })
    };

    let run_time = env::args().skip(1).next()
                                      .and_then(|s| s.parse::<u64>().ok());
    match run_time {
        Some(mut run_time) if run_time > 0 => {
            #[cfg(debug_assertions)]
            println!("click {} seconds", run_time);
            while run_time > 0 && running.load(Ordering::SeqCst) {
                thread::sleep(time::Duration::from_secs(1));
                run_time -= 1;
            }
        },
        _ => {
            #[cfg(debug_assertions)]
            println!("click until pointer moved");
            while running.load(Ordering::SeqCst) {
                thread::sleep(time::Duration::from_millis(CLICK_DELAY));
            }
        },
    }

    running.store(false, Ordering::SeqCst);
    let _ = click_thread.join();
}
