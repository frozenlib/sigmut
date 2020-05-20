use reactive_fn::*;

fn main() {
    let cell = ReCell::new(5);
    let re = cell.to_re().map(|x| x + 1).cached().cloned();

    let _u = re.for_each(|x| {
        println!("{}", x);
        let x = 0;
    });

    cell.set_and_update(7);
    cell.set_and_update(10);
}
