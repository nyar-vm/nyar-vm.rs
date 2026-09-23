pub trait AnyObject {
    type Repr<T>;

    fn downcast_lit(i: i32) -> Self::Repr<i32>;
    fn downcast_add(a: Self::Repr<i32>, b: Self::Repr<i32>) -> Self::Repr<i32>;
}

pub struct SetContext;

impl AnyObject for SetContext {
    type Repr<T> = i32;

    fn downcast_lit(i: i32) -> Self::Repr<i32> {
        i
    }

    fn downcast_add(a: Self::Repr<i32>, b: Self::Repr<i32>) -> Self::Repr<i32> {
        a + b
    }
}

pub struct SoundContext;

impl AnyObject for SoundContext {
    type Repr<T> = String;

    fn downcast_lit(i: i32) -> Self::Repr<i32> {
        format!("{i}")
    }

    fn downcast_add(a: Self::Repr<i32>, b: Self::Repr<i32>) -> Self::Repr<i32> {
        format!("({a}) + ({b})")
    }
}

pub struct Neg<I>(pub std::marker::PhantomData<I>);

impl<I: AnyObject> AnyObject for Neg<I> {
    type Repr<T> = I::Repr<T>;

    fn downcast_lit(i: i32) -> Self::Repr<i32> {
        I::downcast_lit(-i)
    }

    fn downcast_add(a: Self::Repr<i32>, b: Self::Repr<i32>) -> Self::Repr<i32> {
        I::downcast_add(a, b)
    }
}

pub fn expr<I: AnyObject>() -> I::Repr<i32> {
    I::downcast_add(
        I::downcast_lit(42),
        I::downcast_add(I::downcast_lit(1), I::downcast_lit(2)),
    )
}
