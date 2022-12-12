use rusty_fortran::RustyFortranFrontend;
use nyar_types::NyarFrontend;
use oak_vfs::MemoryVfs;

#[test]
fn test_parse_hello() {
    let frontend = RustyFortranFrontend::default();
    let source = r#"program hello
  print *, "Hello, Fortran!"
end program hello
"#;
    let ast = frontend.parse(source).expect("Failed to parse Fortran source");
    let vfs = MemoryVfs::default();
    let _tree = frontend.lower(&ast, &vfs).expect("Failed to lower AST to UIR");
}

#[test]
fn test_loop_and_if() {
    let frontend = RustyFortranFrontend::default();
    let source = r#"program loop_if
  integer :: i, sum
  sum = 0
  do i = 1, 10
    if (i > 5) then
      sum = sum + i
    end if
  end do
  print *, "Sum is:", sum
end program loop_if
"#;
    let ast = frontend.parse(source).expect("Failed to parse Fortran source");
    let vfs = MemoryVfs::default();
    let _tree = frontend.lower(&ast, &vfs).expect("Failed to lower AST to UIR");
}
