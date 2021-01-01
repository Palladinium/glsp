# RFn

As we discussed [earlier](syntax-and-types.md#type-summary), one of the possible types for a
GameLisp value is `rfn`. This type stores a Rust function which can be called from GameLisp; 
`rfn` stands for "Rust function".

In the Rust API, `rfn` is represented by the [`Val::RFn` enum variant] and the [`RFn` struct].
You'll normally access it through a pointer: `Root<RFn>`.

To construct a new `RFn`, simply call [`glsp::rfn`], passing in a reference to an appropriate
Rust function.

```rust
let swap_bytes: Root<RFn> = glsp::rfn(&i32::swap_bytes);
glsp::bind_global("swap-bytes", swap_bytes)?;

//the standard Rust function i32::swap_bytes can now be called from 
//GameLisp code, by invoking the global rfn (swap-bytes)
```

```
(prn (swap-bytes 32768)) ; prints 8388608
(prn (swap-bytes 8388608)) ; prints 32768
```

Because constructing an `RFn` and binding it to a global variable is such a common operation,
we provide [`glsp::bind_rfn`], which performs both steps at once.

```rust
glsp::bind_rfn("swap-bytes", &i32::swap_bytes)?;
```

`glsp::rfn` and `glsp::bind_rfn` don't just accept function pointers - they will also accept
Rust closures, even closures which capture Rust variables.

However, closures passed to `glsp::rfn` must be `'static` and immutable. This means that if the 
closure captures any variables, it must take ownership of those variables using the `move` keyword.

```rust
//a non-capturing closure
glsp::rfn(&|a: i32, b: i32| a.saturating_mul(b + 1));

//a capturing closure which uses interior mutability 
//to update its captured variable
let captured = Cell::new(0_i32);
let print_and_inc = move || {
	println!("{}", captured.get());
	captured.set(captured.get() + 1);
};

glsp::bind_rfn("print-and-inc", Box::new(print_and_inc))?;
```

```
(print-and-inc) ; prints 0
(print-and-inc) ; prints 1
(print-and-inc) ; prints 2
```

Even generic Rust functions can be bound to GameLisp - you just need to explicitly select a single 
concrete type signature. For example, to bind the generic [`fs::rename` function], you might 
specify that both parameters are `Strings`:

```rust
glsp::bind_rfn("rename", &fs::rename::<String, String>)?;
```

As demonstrated above, `glsp::rfn` will silently convert GameLisp values into Rust arguments,
as well as converting Rust return values into GameLisp values. This conversion is automatic
and invisible; the conversion code is automatically generated by Rust's trait system.

My intent is that it should be possible to write at least 90% of your scriptable functions 
and methods in a language-agnostic way, so that they can be called from either Rust or GameLisp 
without any modification. This should also make it easy for you to bind third-party functions, 
like `i32::swap_bytes` and `fs::rename`.

[`Val::RFn` enum variant]: https://docs.rs/glsp/*/glsp/enum.Val.html
[`RFn` struct]: https://docs.rs/glsp/*/glsp/struct.RFn.html
[`glsp::rfn`]: https://docs.rs/glsp/*/glsp/fn.rfn.html
[`glsp::bind_rfn`]: https://docs.rs/glsp/*/glsp/fn.bind_rfn.html
[`fs::rename` function]: https://doc.rust-lang.org/std/fs/fn.rename.html


## Return Value Conversions

For an `rfn`'s return value to be automatically converted into a GameLisp value, it must
implement the [`IntoVal` trait].

`IntoVal` is implemented for most of Rust's primitive types, many of the types in GameLisp's
prelude (such as `Root<Arr>`), and a number of Rust standard library types. See the [rustdoc] 
for the full run-down.

When a function returns `Option<T>`, GameLisp will convert the Rust value `None` into the 
GameLisp value `#n`.

```rust
//u8::checked_add consumes two u8 and 
//returns an Option<u8>
glsp::bind_rfn("checked-add", &u8::checked_add)?;
```

```
(prn (checked-add 150 50)) ; prints 200
(prn (checked-add 250 50)) ; prints #n
```

Functions which return `Result<T>` will correctly propagate errors to the caller, converting
non-GameLisp errors into GameLisp errors when necessary.

```rust
//fs::read_to_string consumes a String by value and
//returns a Result<String, std::io::Error>
glsp::bind_rfn("read-to-string", &fs::read_to_string::<String>)?;
```

```
(ensure (str? (read-to-string "Cargo.toml")))
(ensure (matches?
  (try (read-to-string "does-not-exist.txt"))
  ('err _)))
```

Functions which return various Rust collection types - including tuples, slices, arrays, 
string slices, and paths - will construct a new GameLisp array, string or table.

```rust
fn count_chars(src: &str) -> HashMap<char, usize> {
	let mut char_counts = HashMap::<char, usize>::new();
	for ch in src.chars() {
		*char_counts.entry(ch).or_insert(0) += 1;
	}

	char_counts
}

glsp::bind_rfn("count-chars", &count_chars)?;
```

```
(let char-counts (count-chars "consonance"))
(ensure (tab? char-counts))
(prn (len char-counts)) ; prints 6
```

[`IntoVal` trait]: https://docs.rs/glsp/*/glsp/trait.IntoVal.html
[rustdoc]: https://docs.rs/glsp/*/glsp/trait.IntoVal.html


## Argument Conversions

Arguments are a little more complicated than return values. The full set of automatic argument
conversions is listed in [the rustdoc]; we'll explore some of them in more detail over the
next three chapters.

For now, it's enough for you to know that there's a [`FromVal` trait] which is implemented for 
many Rust and GameLisp types. GameLisp can automatically convert `rfn` arguments into any type 
which implements `FromVal`.

```rust
fn example(integer: u64, string: String, tuple: (i8, i8)) {
	println!("{:?} {:?} {:?}", integer, string, tuple);
}

glsp::bind_rfn("example", &example)?;
```

```
(example 1 "two" '(3 4)) ; prints 1 "two" (3, 4)
```

In addition, automatic argument conversions are provided for a handful of reference types, like 
`&Arr`, `&RData`, `&str`, `&Path` and `&[T]`.

```rust
fn example(string: &str, array: &[Sym]) {
	println!("{:?} {:?}", string, array);
}

glsp::bind_rfn("example", &example)?;
```

```
(example "hello" '(game lisp)) ; prints "hello" [game, lisp]
```

[`FromVal` trait]: https://docs.rs/glsp/*/glsp/trait.FromVal.html
[the rustdoc]: https://docs.rs/glsp/*/glsp/fn.rfn.html


### Optional and Rest Parameters

Parameters of type `Option<T>` are optional. If no value is passed at that position in
the argument list, the argument will default to `None`. It will also be set to `None` if the 
caller passes in `#n`.

If an `rfn`'s final parameter has the type [`Rest<T>`], it will collect any number of 
trailing arguments.

[`Rest<T>`]: https://docs.rs/glsp/*/struct.Rest.html

```rust
fn example(non_opt: u8, opt: Option<u8>, rest: Rest<u8>) {
	prn!("{:?} {:?} {:?}", non_opt, opt, &*rest);
}

glsp::bind_rfn("example", &example)?;
```

```
(prn (min-args example)) ; prints 1
(prn (max-args example)) ; prints #n

(example)          ; error: too few arguments
(example 1)        ; prints 1 None []
(example 1 2)      ; prints 1 Some(2) []
(example 1 2 3)    ; prints 1 Some(2) [3]
(example 1 2 3 4)  ; prints 1 Some(2) [3, 4]
(example 1 #n 3 4) ; prints 1 None [3, 4]
```


## Custom Conversions

It's possible to implement `IntoVal` and `FromVal` for your own Rust types. This will enable 
your Rust types to participate in automatic conversions when they're used as an argument or 
return value.

For example, it often makes sense to represent a Rust enum as a GameLisp symbol:

```rust
#[derive(Copy, Clone)]
enum Activity {
	Rest,
	Walk,
	Fight
}

impl IntoVal for Activity {
	fn into_val(self) -> GResult<Val> {
		let sym = match self {
			Activity::Rest => sym!("rest"),
			Activity::Walk => sym!("walk"),
			Activity::Fight => sym!("fight")
		};

		sym.into_val()
	}
}

impl FromVal for Activity {
	fn from_val(val: &Val) -> GResult<Self> {
		Ok(match *val {
			Val::Sym(s) if s == sym!("rest") => Activity::Rest,
			Val::Sym(s) if s == sym!("walk") => Activity::Walk,
			Val::Sym(s) if s == sym!("fight") => Activity::Fight,
			ref val => bail!("expected an Activity, received {}", val)
		})
	}
}

impl Activity {
	fn energy_cost(self) -> i32 {
		match self {
			Activity::Rest => 1,
			Activity::Walk => 5,
			Activity::Fight => 25
		}
	}
}

glsp::bind_rfn("energy-cost", &Activity::energy_cost)?;
```

```
(prn (energy-cost 'rest)) ; prints 1
(prn (energy-cost 'fight)) ; prints 25
(prn (energy-cost 'sprint)) ; type conversion error
```

You might also consider representing tuple structs as GameLisp arrays. For example, the
tuple struct `Rgb(128, 64, 32)` could be represented in GameLisp as an array of three
integers, `(128 64 32)`.

[primitive types]: syntax-and-types.md#type-summary


## Errors

To return a GameLisp error from an `rfn`, you can simply set the function's return type to 
[`GResult<T>`], which is an alias for `Result<T, GError>`.

The usual way to trigger a GameLisp error is using the macros [`bail!()`] and [`ensure!()`].
`bail` constructs a new `GError` and returns it. `ensure` tests a condition and calls `bail` 
when the condition is false. (The names of these macros are conventional in Rust error-handling
libraries, such as [`error-chain`] and [`failure`].)

If you need to create an error manually, you can use the [`error!()`] macro, or one of 
[`GError`]'s constructors. An arbitrary [`Error`] type can be reported as the cause of 
an error using the [`with_source`] method.

```rust
fn file_to_nonempty_string(path: &Path) -> GResult<String> {
	match std::fs::read_to_string(path) {
		Ok(st) => {
			ensure!(st.len() > 0, "empty string in file {}", path);
			Ok(st)
		}
		Err(io_error) => {
			let glsp_error = error!("failed to open the file {}", path);
			Err(glsp_error.with_source(io_error))
		}
	}
}
```

If a panic occurs within an `rfn`'s dynamic scope, the panic will be [caught] by the innermost 
`rfn` call and converted into a `GResult`. The panic will still print its usual message to stderr. 
If this is undesirable, you can override the default printing behaviour with a [custom panic hook].

[`bail!()`]: https://docs.rs/glsp/*/glsp/macro.bail.html
[`ensure!()`]: https://docs.rs/glsp/*/glsp/macro.ensure.html
[`error!()`]: https://docs.rs/glsp/*/glsp/macro.error.html
[`GResult`]: https://docs.rs/glsp/*/glsp/type.GResult.html
[`GResult<T>`]: https://docs.rs/glsp/*/glsp/type.GResult.html
[`GError`]: https://docs.rs/glsp/*/glsp/struct.GError.html
[`Error`]: https://doc.rust-lang.org/std/error/trait.Error.html
[`with_source`]: https://docs.rs/glsp/*/glsp/struct.GError.html#method.with_source
[caught]: https://doc.rust-lang.org/std/panic/fn.catch_unwind.html
[custom panic hook]: https://doc.rust-lang.org/std/panic/fn.set_hook.html
[`error-chain`]: https://docs.rs/error-chain/0.12.2/error_chain/
[`failure`]: https://docs.rs/failure/0.1.8/failure/


## `RFn` Macros

Both Rust functions and GameLisp functions can be used as macros (although GameLisp functions 
are usually the preferred choice).

Within a Rust function, [`macro_no_op!()`] will create and return a special kind of `GError` 
which suppresses further expansion of the current macro. (Incidentally, this is also how the
[`macro-no-op`](../std/macro-no-op) built-in function works.)

This means that you can only use [`macro_no_op!()`] in a function which returns [`GResult`].

[`macro_no_op!()`]: https://docs.rs/glsp/*/glsp/macro.macro_no_op.html


## Limitations

GameLisp's automatic function-binding machinery pushes Rust's type system to its limit. To make
it work, I've had to explore some obscure corners of the trait system. Unfortunately, this has led 
to a few tricky limitations.

Due to [`rustc` bug #79207](https://github.com/rust-lang/rust/issues/79207), it's not
possible to pass a function pointer or closure to `glsp::rfn` by value; it will cause a
type-inference error. Instead, functions should be passed by reference, and capturing
closures should be wrapped in a call to `Box::new`.

Due to [`rustc` bug #70263](https://github.com/rust-lang/rust/issues/70263), some functions
which return non-`'static` references can't be passed to `glsp::rfn`, even when the function's 
return type implements `IntoVal`. This usually occurs with function signatures which both
consume and return a reference, like `fn(&str) -> &str`.

`glsp::rfn` won't accept Rust functions with more than eight parameters. If necessary, you can 
work around this by capturing any number of trailing arguments as a `Rest<T>`, and unpacking 
those arguments manually:

```rust
fn process_ten_integers(
	arg0: i32,
	arg1: i32,
	arg2: i32,
	arg3: i32,
	arg4: i32,
	arg5: i32,
	arg6: i32,
	rest: Rest<i32>
) -> GResult<()> {

	ensure!(
		rest.len() == 3,
		"expected exactly 10 arguments, but received {}",
		7 + rest.len()
	);

	let [arg7, arg8, arg9] = [rest[0], rest[1], rest[2]];

	//...
}
```

We use [specialization] internally. To implement the `IntoVal`, `FromVal` or `RGlobal` traits for 
your own types, you'll need to enable the nightly feature `min_specialization`, by placing this 
attribute towards the top of your `main.rs` or `lib.rs` file:

[specialization]: https://github.com/rust-lang/rfcs/blob/master/text/1210-impl-specialization.md

```rust
#![feature(min_specialization)]
```

Finally, many `rfn` parameter types which are accepted by value can't be accepted by reference.
`String` and `i32` are fine, but `&String` and `&mut i32` aren't.