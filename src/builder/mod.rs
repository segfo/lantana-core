pub mod thread_observer;

#[derive(Debug)]
pub struct BuilderError{
    kind:BuilderErrorKind
}

impl BuilderError{
    pub fn new()->Self{
        BuilderError{kind:BuilderErrorKind::None}
    }
}
#[derive(Debug)]
enum BuilderErrorKind{
    None,InvalidParameter
}

impl std::fmt::Display for BuilderError{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.kind {
            BuilderErrorKind::None=>write!(f, "No error"),
            BuilderErrorKind::InvalidParameter=>write!(f, "It may not satisfy the parameter requirement required for constructing the structure.")
        }
    }
}

impl std::error::Error for BuilderError{
    fn description(&self) -> &str{
        match self.kind{
            BuilderErrorKind::None=>"No error",
            BuilderErrorKind::InvalidParameter=>"Invalid parameter"
        }
    }
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)>{
        None
    }
}
