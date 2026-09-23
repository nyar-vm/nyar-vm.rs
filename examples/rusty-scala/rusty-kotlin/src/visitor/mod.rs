pub trait AnyObject {
    fn downcast(&self, visitor: &mut dyn AnyDowncast);
}

pub trait Animal: AnyObject {}
pub struct Dog {
    pub name: String,
}
pub struct Cat {
    pub name: String,
}

pub struct Fish {
    pub name: String,
}
/// All downcast call sites
///
/// AnyDowncast is a virtual table that stores all the downcast functions needed by the program.
///
/// Obviously, this is very large.
pub trait AnyDowncast {
    fn downcast_dog(&mut self, dog: &Dog) {
        self.otherwise()
    }
    fn downcast_cat(&mut self, cat: &Cat) {
        self.otherwise()
    }
    fn downcast_fish(&mut self, fish: &Fish) {
        self.otherwise()
    }
    fn otherwise(&mut self) {}
}

impl AnyObject for Dog {
    fn downcast(&self, visitor: &mut dyn AnyDowncast) {
        visitor.downcast_dog(self);
    }
}
impl AnyObject for Cat {
    fn downcast(&self, visitor: &mut dyn AnyDowncast) {
        visitor.downcast_cat(self);
    }
}
impl AnyObject for Fish {
    fn downcast(&self, visitor: &mut dyn AnyDowncast) {
        visitor.downcast_fish(self);
    }
}
impl Animal for Dog {}
impl Animal for Cat {}
impl Animal for Fish {}

pub struct SoundContext {
    pub name: String,
}
impl AnyDowncast for SoundContext {
    fn downcast_dog(&mut self, dog: &Dog) {
        self.name = dog.name.clone();
        println!("Woof!");
    }

    fn downcast_cat(&mut self, cat: &Cat) {
        self.name = cat.name.clone();
        println!("Meow!");
    }
    fn otherwise(&mut self) {
        println!("What?");
    }
}

pub fn sound(animal: &dyn Animal) -> SoundContext {
    let mut ctx = SoundContext {
        name: "".to_string(),
    };
    animal.downcast(&mut ctx);
    ctx
}
