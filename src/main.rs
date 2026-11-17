mod camera;
#[cfg(test)]
mod tests;
mod window;
mod world;

fn main() {
    if let Err(err) = window::run() {
        eprintln!("ошибка: {err}");
    }
}
