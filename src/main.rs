use reactive_fn::*;

fn main() {
    let cell = ReCell::new(5);
    let re = cell.to_re().dedup().cloned();

    let _u = re.for_each(|x| {
        println!("{}", x);
        //
    });

    cell.set(5);
    cell.set(5);
    cell.set(6);
    cell.set(6);
    cell.set(5);
}
