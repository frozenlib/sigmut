use reactive_fn::*;

fn main() {
    let cell = ReCell::new(5);
    let re = cell.to_re().dedup().cloned();

    let _u = re.for_each(|x| {
        println!("{}", x);
        //
    });

    cell.set_and_update(5);
    cell.set_and_update(5);
    cell.set_and_update(6);
    cell.set_and_update(6);
    cell.set_and_update(5);
}
