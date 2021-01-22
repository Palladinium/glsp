use super::engine::{glsp, with_vm, Guard, Span};
use super::val::Val;
use super::vm::Frame;
use super::wrap::IntoVal;
use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter};

//this isn't picked up properly by rustdoc, due to rustc bug 80557 (todo: remove this note once
//the bug is fixed). moving the definition of GResult to the root of the `glsp` crate was not
//an effective workaround
pub type GResult<T> = Result<T, GError>;

/**
The error type generated by GameLisp code.

[`GResult<T>`](type.GResult.html) is an alias for `Result<T, GError>`.

The easiest way to generate an error yourself is by using one of the macros
[`bail!`](macro.bail.html), [`ensure!`](macro.ensure.html) or [`error!`](macro.error.html).

The [`macro-no-op`](https://gamelisp.rs/std/macro-no-op) signal is represented by a special kind
of `GError` which is not caught by [`try`](https://gamelisp.rs/std/try) or
[`try-verbose`](https://gamelisp.rs/std/try-verbose). This means that in order to trigger a
[`macro_no_op!`](macro.macro_no_op.html), the enclosing function must return `GResult<T>`.

The [`with_source` method](#method.with_source) can be used to chain together two `GErrors`,
or to chain an arbitrary [`Error`](https://doc.rust-lang.org/std/error/trait.Error.html) type
onto a `GError`.
*/

pub struct GError {
    pub(crate) payload: Box<Payload>,
}

//we separate the GError from its Payload to make GResult several words smaller. the
//only trade-off is one extra allocation when an error does occur.
pub(crate) enum Payload {
    Error {
        val: Val,
        file_location: Option<String>,
        stack_trace: Option<String>,

        defer_chain: Option<GError>,
        source: Option<Box<dyn Error + 'static>>,
    },
    MacroNoOp,
}

impl GError {
    pub fn new() -> GError {
        GError::from_str("explicit call to bail!, error!, or GError::new")
    }

    pub fn from_str(st: &str) -> GError {
        GError::from_val(st)
    }

    pub fn from_val<T: IntoVal>(t: T) -> GError {
        let val = t.into_val().unwrap_or(Val::Nil);
        let file_location = glsp::file_location();
        let stack_trace = if glsp::errors_verbose() {
            Some(glsp::stack_trace())
        } else {
            None
        };

        GError {
            payload: Box::new(Payload::Error {
                val,
                file_location,
                stack_trace,
                defer_chain: None,
                source: None,
            }),
        }
    }

    pub fn macro_no_op() -> GError {
        with_vm(|vm| {
            if vm.in_expander() {
                GError {
                    payload: Box::new(Payload::MacroNoOp),
                }
            } else {
                GError::from_str("(macro-no-op) called outside of any macro expander")
            }
        })
    }

    /**
    Returns `true` if this error was generated using [`macro_no_op!`](macro.macro_no_op.html) or
    [`GError::macro_no_op`](#method.macro_no_op).
    */
    pub fn is_macro_no_op(&self) -> bool {
        match &*self.payload {
            Payload::MacroNoOp => true,
            Payload::Error { .. } => false,
        }
    }

    /**
    Returns the error's payload. Panics if this error is a macro-no-op.
    */
    pub fn val(&self) -> Val {
        match &*self.payload {
            Payload::MacroNoOp => panic!(),
            Payload::Error { val, .. } => val.clone(),
        }
    }

    /**
    Returns the error's saved stack trace.

    Errors invoked in a dynamic context where verbose errors are disabled (for example,
    the dynamic scope of a [`try` form](https://gamelisp.rs/std/try)) will not have a
    stack trace.
    */
    pub fn stack_trace(&self) -> Option<&str> {
        match &*self.payload {
            Payload::MacroNoOp => panic!(),
            Payload::Error { stack_trace, .. } => stack_trace.as_ref().map(|s| &**s),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn defer_chain(&self) -> Option<&GError> {
        match &*self.payload {
            Payload::MacroNoOp => panic!(),
            Payload::Error { defer_chain, .. } => defer_chain.as_ref(),
        }
    }

    pub(crate) fn chain_defer_error(&mut self, defer_error: GError) {
        if self.is_macro_no_op() {
            *self = defer_error;
        } else {
            match &mut *self.payload {
                Payload::Error {
                    ref mut defer_chain,
                    ..
                } => {
                    if let Some(ref mut defer_chain) = defer_chain {
                        defer_chain.chain_defer_error(defer_error);
                    } else {
                        *defer_chain = Some(defer_error);
                    }
                }
                Payload::MacroNoOp => unreachable!(),
            }
        }
    }

    /**
    Chains another error onto this `GError`.

    This can be used to wrap arbitrary [`Error` types][0] in a `GError`.

    [0]: https://doc.rust-lang.org/std/error/trait.Error.html

        let words = match fs::read_to_string("words.txt") {
            Ok(words) => words,
            Err(fs_err) => {
                return Err(error!("failed to open words.txt").with_source(fs_err))
            }
        };
    */
    pub fn with_source(mut self, source_to_add: impl Error + 'static) -> GError {
        match &mut *self.payload {
            Payload::MacroNoOp => panic!(),
            Payload::Error { source, .. } => *source = Some(Box::new(source_to_add)),
        }

        self
    }

    #[doc(hidden)]
    pub fn new_at(span: Span) -> GError {
        glsp::push_frame(Frame::ErrorAt(span));
        let _guard = Guard::new(|| glsp::pop_frame());

        GError::new()
    }

    #[doc(hidden)]
    pub fn from_str_at(span: Span, st: &str) -> GError {
        glsp::push_frame(Frame::ErrorAt(span));
        let _guard = Guard::new(|| glsp::pop_frame());

        GError::from_str(st)
    }

    #[doc(hidden)]
    pub fn from_val_at<T: IntoVal>(span: Span, t: T) -> GError {
        glsp::push_frame(Frame::ErrorAt(span));
        let _guard = Guard::new(|| glsp::pop_frame());

        GError::from_val(t)
    }
}

impl Error for GError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        if let Payload::Error {
            source: Some(ref source),
            ..
        } = *self.payload
        {
            Some(source.as_ref())
        } else {
            None
        }
    }
}

impl Debug for GError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl Display for GError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match &*self.payload {
            Payload::MacroNoOp => panic!(),
            Payload::Error {
                val,
                file_location,
                stack_trace,
                source,
                defer_chain,
            } => {
                match (file_location, stack_trace) {
                    (&None, &None) => {
                        write!(f, "{:?}", val)
                    }
                    (&Some(ref file_location), &None) => {
                        write!(f, "{}: {:?}", file_location, val)
                    }
                    (_, &Some(ref stack_trace)) => {
                        write!(f, "stack trace:\n")?;
                        for line in stack_trace.lines() {
                            write!(f, "    {}\n", line)?;
                        }

                        //we print error values using {} rather than {:?}. this is because most
                        //error messages are strings, and escaping curly braces can be confusing.
                        //"clause must end with }" becomes "clause must end with }}".
                        if let Some(ref source) = *source {
                            write!(f, "\nerrors:")?;

                            fn write_source(
                                f: &mut Formatter,
                                source: &(dyn Error + 'static),
                            ) -> fmt::Result {
                                //if the source is a GError, we can't stringify it using {},
                                //because that would print its full stack trace again
                                match source.downcast_ref::<GError>() {
                                    Some(error) => write!(f, "\n    {}", error.val())?,
                                    None => write!(f, "\n    {}", source)?,
                                }

                                if let Some(source2) = source.source() {
                                    write_source(f, source2)
                                } else {
                                    Ok(())
                                }
                            }

                            write!(f, "\n    {}", &val)?;
                            write_source(f, source.as_ref())?;
                        } else {
                            write!(f, "\nerror: {}", &val)?;
                        }

                        if let Some(ref defer_chain) = defer_chain {
                            write!(
                                f,
                                "\n\nwhile this error was unwinding, \
                                     a (defer) form also failed:\n"
                            )?;
                            write!(f, "\n{}", defer_chain)?;
                        }

                        Ok(())
                    }
                }
            }
        }
    }
}

/**
Constructs a [`GError`](struct.GError.html) by formatting a string.

This is usually more convenient than calling `GError`'s constructors directly.

`error!()` is equivalent to [`GError::new()`][0].

[0]: struct.GError.html#method.new

`error!(x)` is equivalent to [`GError::from_val(x)`][1].

[1]: struct.GError.html#method.from_val

`error!("{}", a)` is equivalent to [`GError::from_str(&format!("{}", a))`][2].

[2]: struct.GError.html#method.from_str
*/

#[macro_export]
macro_rules! error {
    () => ($crate::GError::new());
    ($val:expr) => ($crate::GError::from_val($val));
    ($fmt:literal, $($arg:tt)+) => ($crate::GError::from_str(&format!($fmt, $($arg)+)));
}

#[doc(hidden)]
#[macro_export]
macro_rules! error_at {
    ($span:expr) => ($crate::GError::new_at($span));
    ($span:expr, $val:expr) => ($crate::GError::from_val_at($span, $val));
    ($span:expr, $fmt:literal, $($arg:tt)+) => (
        $crate::GError::from_str_at($span, &format!($fmt, $($arg)+))
    );
}

/**
Constructs a [`GError`](struct.GError.html) and returns it.

The arguments to `bail!` are the same as the arguments to [`error!`](macro.error.html).
For example, `bail!(x)` is equivalent to:

    return Err(GError::from_val(x))
*/

#[macro_export]
macro_rules! bail {
    ($($arg:tt)*) => (return Err($crate::error!($($arg)*)));
}

#[doc(hidden)]
#[macro_export]
macro_rules! bail_at {
    ($span:expr) => (return Err($crate::error_at!($span)));
    ($span:expr, $($arg:tt)+) => (return Err($crate::error_at!($span, $($arg)*)));
}

/**
Tests a condition, returning an error if the result is `false`.

The first argument must be an expression of type `bool`. If it evaluates to `false`,
the remaining arguments are all passed to [`bail!`](macro.bail.html). If it evaluates to `true`,
those arguments aren't evaluated at all.
*/

#[macro_export]
macro_rules! ensure {
    ($condition:expr) => (
        if !($condition) {
            bail!("ensure!({}) failed", stringify!($condition))
        }
    );
    ($condition:expr, $($arg:tt)*) => (
        if !($condition) {
            bail!($($arg)*)
        }
    );
}

#[doc(hidden)]
#[macro_export]
macro_rules! ensure_at {
    ($span:expr, $condition:expr) => (
        if !($condition) {
            bail_at!($span, "ensure!({}) failed", stringify!($condition))
        }
    );
    ($span:expr, $condition:expr, $($arg:tt)*) => (
        if !($condition) {
            bail_at!($span, $($arg)*)
        }
    );
}

/**
Constructs a [`GError`](struct.GError.html) which represents a macro-no-op, and returns it.

`macro_no_op!()` is equivalent to:

    return Err(GError::macro_no_op())
*/

#[macro_export]
macro_rules! macro_no_op {
    () => {
        return Err($crate::GError::macro_no_op())
    };
}
