//! Checks shared by the WGSL Celesta assembles around someone else's code:
//! custom shaders (`shader.rs`) and scene materials (`scene_material.rs`).
//! Each is one module of Celesta's own declarations, the author's code at
//! a known byte range, and Celesta's entry points after it.

use std::fmt::Write as _;
use std::ops::Range;

/// `message`, located at byte `at` of `code`: as a line and column of the
/// author's code (the `author` range) when it lies there, followed by
/// `before` when it lies in Celesta's declarations before that code, or by
/// `after` when it lies in the entry points after it. An empty hint adds
/// nothing.
pub(crate) fn locate(
    code: &str,
    author: &Range<usize>,
    at: Option<usize>,
    message: String,
    before: &str,
    after: &str,
) -> String {
    let hinted = |hint: &str| {
        if hint.is_empty() {
            message.clone()
        } else {
            format!("{message} {hint}")
        }
    };
    match at {
        Some(at) if author.contains(&at) || (at == author.end && !author.is_empty()) => {
            let before = &code[author.start..at];
            let line = before.matches('\n').count() + 1;
            let column = before.len() - before.rfind('\n').map_or(0, |newline| newline + 1) + 1;
            format!("{line}:{column}: {message}")
        }
        Some(at) if at < author.start => hinted(before),
        Some(at) if at > author.end => hinted(after),
        _ => message,
    }
}

/// `error` and the errors that caused it, outermost first.
pub(crate) fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(error) = cause {
        write!(message, ": {error}").expect("writing to a String cannot fail");
        cause = error.source();
    }
    message
}

/// What `check_declarations` allows the author's code.
pub(crate) struct Rules<'a> {
    /// Names the author's code in messages: "a custom shader".
    pub(crate) what: &'a str,
    /// Whether Celesta's declarations bind a resource of this name.
    pub(crate) prelude_resource: &'a dyn Fn(&str) -> bool,
    /// The entry points that are Celesta's own.
    pub(crate) entry_points: &'a [&'a str],
    /// Follows the refusal of an entry point of the author's: what the
    /// author defines instead.
    pub(crate) instead: &'a str,
    /// Whether a name belongs to Celesta, so the author cannot declare it.
    pub(crate) reserved: &'a dyn Fn(&str) -> bool,
}

/// Rejects entry points, resources, and workgroup variables the author
/// declared, and names `rules.reserved` keeps for Celesta: Celesta's
/// declarations bind everything the author's code gets.
pub(crate) fn check_declarations(
    module: &naga::Module,
    code: &str,
    author: &Range<usize>,
    rules: &Rules,
) -> Result<(), String> {
    let start = |span: naga::Span| span.to_range().map(|range| range.start);
    let fail = |at: Option<usize>, message: String| Err(locate(code, author, at, message, "", ""));
    let what = rules.what;
    for entry in &module.entry_points {
        if !rules.entry_points.contains(&entry.name.as_str()) {
            // naga keeps no span for an entry point; its `fn` is in the
            // author's code.
            let at = code[author.clone()]
                .find(&format!("fn {}", entry.name))
                .map(|offset| author.start + offset);
            return fail(
                at,
                format!(
                    "declares the entry point `{}`; {}",
                    entry.name, rules.instead
                ),
            );
        }
    }
    for (handle, global) in module.global_variables.iter() {
        let name = global.name.as_deref().unwrap_or("");
        let at = start(module.global_variables.get_span(handle));
        // Told apart by where they are declared, not by name: the author
        // could otherwise bind a resource of a prelude's name that this
        // module does not declare.
        let authored = at.is_none_or(|at| author.contains(&at));
        if global.binding.is_some() && (authored || !(rules.prelude_resource)(name)) {
            return fail(
                at,
                format!("declares the resource `{name}`; {what} cannot bind resources of its own"),
            );
        }
        // naga's validator lets every stage use one, so a backend's
        // compiler or driver meets it in a vertex or fragment shader.
        if global.space == naga::AddressSpace::WorkGroup {
            return fail(
                at,
                format!(
                    "declares the workgroup variable `{name}`; workgroup memory is for compute shaders, and {what} runs in the vertex and fragment stages"
                ),
            );
        }
    }
    // Celesta's own declarations lie outside the author's code. Read from
    // naga's module, then from the source for what the module keeps no
    // name of (aliases). The author's entry points need no name check:
    // every one that is not Celesta's was refused above.
    let reserved = |at: Option<usize>, name: &str| {
        fail(
            at,
            format!("declares `{name}`; names starting with `celesta` belong to Celesta"),
        )
    };
    for (name, at) in module_names(module, author) {
        if (rules.reserved)(name) {
            return reserved(Some(at), name);
        }
    }
    for (offset, name) in module_declarations(&code[author.clone()]) {
        if (rules.reserved)(name) {
            return reserved(Some(author.start + offset), name);
        }
    }
    Ok(())
}

/// The names of `module`'s functions, global variables, constants,
/// overrides, and named types declared in the `author` range of its
/// source, each with the byte where its declaration starts. naga keeps no
/// alias names, nor constants of abstract type, which it folds into their
/// uses; `module_declarations` reads both from the source. (Calling a
/// constant is a parse error, so one named like a built-in function or
/// type cannot change what a call means.)
pub(crate) fn module_names<'a>(
    module: &'a naga::Module,
    author: &Range<usize>,
) -> Vec<(&'a str, usize)> {
    let functions = module
        .functions
        .iter()
        .map(|(handle, function)| (function.name.as_deref(), module.functions.get_span(handle)));
    let globals = module.global_variables.iter().map(|(handle, global)| {
        (
            global.name.as_deref(),
            module.global_variables.get_span(handle),
        )
    });
    let constants = module
        .constants
        .iter()
        .map(|(handle, constant)| (constant.name.as_deref(), module.constants.get_span(handle)));
    let overrides = module
        .overrides
        .iter()
        .map(|(handle, constant)| (constant.name.as_deref(), module.overrides.get_span(handle)));
    let types = module
        .types
        .iter()
        .map(|(handle, ty)| (ty.name.as_deref(), module.types.get_span(handle)));
    functions
        .chain(globals)
        .chain(constants)
        .chain(overrides)
        .chain(types)
        .filter_map(|(name, span)| {
            let at = span.to_range()?.start;
            (author.contains(&at)).then_some((name?, at))
        })
        .collect()
}

/// naga's line break, which ends a line comment: Unicode
/// Pattern_White_Space but for the space, the tab, U+200E, and U+200F.
const fn is_comment_end(c: char) -> bool {
    matches!(
        c,
        '\u{000a}'..='\u{000d}' | '\u{0085}' | '\u{2028}' | '\u{2029}'
    )
}

/// naga's blankspace: Unicode Pattern_White_Space.
const fn is_blankspace(c: char) -> bool {
    matches!(
        c,
        '\u{0020}' | '\u{0009}'
            ..='\u{000d}' | '\u{0085}' | '\u{200e}' | '\u{200f}' | '\u{2028}' | '\u{2029}'
    )
}

/// The names `source` declares at module scope (functions, constants,
/// overrides, variables, structs, and aliases), each with the byte where
/// it starts. Read from the source rather than from naga's module, which
/// keeps no alias names.
///
/// Comments, blankspace, and words follow naga 30's lexer
/// (`front/wgsl/parse/lexer.rs`). Numbers are only skipped, up to the next
/// character that is neither a point nor XID_Continue; naga's number
/// grammar can end one sooner, but valid WGSL never puts a word straight
/// after a number, and source that is not valid fails to parse anyway.
pub(crate) fn module_declarations(source: &str) -> Vec<(usize, &str)> {
    let mut declarations = Vec::new();
    let mut depth = 0usize;
    // After a declaring keyword: the name comes next, once a `var`'s
    // template list (`<` … `>`) has closed.
    let mut pending = false;
    let mut angle = 0usize;
    let mut chars = source.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if is_blankspace(c) {
            continue;
        }
        if c == '/' && chars.peek().is_some_and(|&(_, next)| next == '/') {
            // Up to and including the line break.
            while chars.next().is_some_and(|(_, next)| !is_comment_end(next)) {}
            continue;
        }
        if c == '/' && chars.peek().is_some_and(|&(_, next)| next == '*') {
            chars.next();
            // Block comments nest. As in naga, a `*/` or `/*` pair is
            // consumed whole, so its characters pair with nothing else.
            let mut nested = 1usize;
            while nested > 0 {
                match chars.next() {
                    Some((_, '*')) if chars.next_if(|&(_, next)| next == '/').is_some() => {
                        nested -= 1;
                    }
                    Some((_, '/')) if chars.next_if(|&(_, next)| next == '*').is_some() => {
                        nested += 1;
                    }
                    Some(_) => {}
                    None => break,
                }
            }
            continue;
        }
        // A number declares nothing; its digits, letters, and points are
        // skipped together. naga also starts one at a point before a
        // digit, which this reads as the point, then a number.
        if c.is_ascii_digit() {
            while chars
                .next_if(|&(_, next)| next == '.' || unicode_ident::is_xid_continue(next))
                .is_some()
            {}
            continue;
        }
        // naga's lexer's rule for a word: `_` or XID_Start, then
        // XID_Continue.
        if c == '_' || unicode_ident::is_xid_start(c) {
            let mut end = at + c.len_utf8();
            while let Some((next_at, next)) =
                chars.next_if(|&(_, next)| unicode_ident::is_xid_continue(next))
            {
                end = next_at + next.len_utf8();
            }
            let word = &source[at..end];
            if depth > 0 {
                continue;
            }
            if pending && angle == 0 {
                declarations.push((at, word));
                pending = false;
            } else if !pending
                && matches!(
                    word,
                    "fn" | "const" | "override" | "var" | "struct" | "alias"
                )
            {
                pending = true;
            }
            continue;
        }
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            '<' if pending => angle += 1,
            '>' if pending => angle = angle.saturating_sub(1),
            _ => {}
        }
    }
    declarations
}

/// Whether WGSL (or naga) predeclares `name` as a built-in function or
/// type. naga resolves a name to the module's own declaration before a
/// built-in, so code that declares one of these changes what every call of
/// it in the module means.
pub(crate) fn predeclared(name: &str) -> bool {
    let vector = |rest: &str| {
        let mut rest = rest.chars();
        rest.next().is_some_and(|size| matches!(size, '2'..='4'))
            && matches!(rest.as_str(), "" | "i" | "u" | "f" | "h")
    };
    let matrix = |rest: &str| {
        let bytes = rest.as_bytes();
        bytes.len() >= 3
            && matches!(bytes[0], b'2'..=b'4')
            && bytes[1] == b'x'
            && matches!(bytes[2], b'2'..=b'4')
            && matches!(&rest[3..], "" | "f" | "h")
    };
    name.strip_prefix("vec").is_some_and(vector)
        || name.strip_prefix("mat").is_some_and(matrix)
        || PREDECLARED.contains(&name)
}

/// The built-in functions and the types other than vectors and matrices
/// that WGSL predeclares, with naga's own extensions.
#[rustfmt::skip]
const PREDECLARED: &[&str] = &[
    // Types.
    "bool", "i32", "u32", "f32", "f16", "i64", "u64", "f64", "i16", "u16", "array", "atomic",
    "ptr", "sampler", "sampler_comparison", "binding_array", "acceleration_structure",
    "ray_query", "RayDesc", "RayIntersection", "coop_mat8x8", "coop_mat16x16", "texture_1d",
    "texture_2d", "texture_2d_array", "texture_3d", "texture_cube", "texture_cube_array",
    "texture_multisampled_2d", "texture_depth_multisampled_2d", "texture_external",
    "texture_storage_1d", "texture_storage_2d", "texture_storage_2d_array",
    "texture_storage_3d", "texture_depth_2d", "texture_depth_2d_array", "texture_depth_cube",
    "texture_depth_cube_array",
    // Bit reinterpretation, logical, and array functions.
    "bitcast", "all", "any", "select", "arrayLength",
    // Numeric functions.
    "abs", "acos", "acosh", "asin", "asinh", "atan", "atanh", "atan2", "ceil", "clamp", "cos",
    "cosh", "countLeadingZeros", "countOneBits", "countTrailingZeros", "cross", "degrees",
    "determinant", "distance", "dot", "dot4U8Packed", "dot4I8Packed", "exp", "exp2",
    "extractBits", "faceForward", "firstLeadingBit", "firstTrailingBit", "floor", "fma",
    "fract", "frexp", "insertBits", "inverseSqrt", "ldexp", "length", "log", "log2", "max",
    "min", "mix", "modf", "normalize", "pow", "quantizeToF16", "radians", "reflect", "refract",
    "reverseBits", "round", "saturate", "sign", "sin", "sinh", "smoothstep", "sqrt", "step",
    "tan", "tanh", "transpose", "trunc",
    // Derivative functions.
    "dpdx", "dpdxCoarse", "dpdxFine", "dpdy", "dpdyCoarse", "dpdyFine", "fwidth",
    "fwidthCoarse", "fwidthFine",
    // Texture functions.
    "textureDimensions", "textureGather", "textureGatherCompare", "textureLoad",
    "textureNumLayers", "textureNumLevels", "textureNumSamples", "textureSample",
    "textureSampleBias", "textureSampleCompare", "textureSampleCompareLevel",
    "textureSampleGrad", "textureSampleLevel", "textureSampleBaseClampToEdge", "textureStore",
    "textureAtomicAdd", "textureAtomicAnd", "textureAtomicMax", "textureAtomicMin",
    "textureAtomicOr", "textureAtomicXor",
    // Atomic functions.
    "atomicLoad", "atomicStore", "atomicAdd", "atomicSub", "atomicMax", "atomicMin",
    "atomicAnd", "atomicOr", "atomicXor", "atomicExchange", "atomicCompareExchangeWeak",
    // Packing functions.
    "pack4x8snorm", "pack4x8unorm", "pack4xI8", "pack4xU8", "pack4xI8Clamp", "pack4xU8Clamp",
    "pack2x16snorm", "pack2x16unorm", "pack2x16float", "unpack4x8snorm", "unpack4x8unorm",
    "unpack4xI8", "unpack4xU8", "unpack2x16snorm", "unpack2x16unorm", "unpack2x16float",
    // Synchronization functions.
    "storageBarrier", "textureBarrier", "workgroupBarrier", "workgroupUniformLoad",
    "subgroupBarrier",
    // Subgroup and quad functions.
    "subgroupAdd", "subgroupExclusiveAdd", "subgroupInclusiveAdd", "subgroupAll",
    "subgroupAnd", "subgroupAny", "subgroupBallot", "subgroupBroadcast",
    "subgroupBroadcastFirst", "subgroupElect", "subgroupMax", "subgroupMin", "subgroupMul",
    "subgroupExclusiveMul", "subgroupInclusiveMul", "subgroupOr", "subgroupShuffle",
    "subgroupShuffleDown", "subgroupShuffleUp", "subgroupShuffleXor", "subgroupXor",
    "quadBroadcast", "quadSwapDiagonal", "quadSwapX", "quadSwapY",
    // naga's ray query and cooperative matrix functions.
    "rayQueryInitialize", "rayQueryProceed", "rayQueryTerminate",
    "rayQueryConfirmIntersection", "rayQueryGenerateIntersection",
    "rayQueryGetCandidateIntersection", "rayQueryGetCommittedIntersection",
    "getCandidateHitVertexPositions", "getCommittedHitVertexPositions", "traceRay", "coopLoad",
    "coopLoadT", "coopStore", "coopStoreT", "coopMultiplyAdd",
];

/// The byte where `error`, from `naga::front::wgsl::parse_str`, points.
pub(crate) fn parse_error_at(error: &naga::front::wgsl::ParseError) -> Option<usize> {
    error
        .labels()
        .next()
        .and_then(|(span, _)| span.to_range())
        .map(|range| range.start)
}

/// The byte where `error`, from naga's validator, points.
pub(crate) fn validation_error_at(
    error: &naga::WithSpan<naga::valid::ValidationError>,
) -> Option<usize> {
    error
        .spans()
        .next()
        .and_then(|(span, _)| span.to_range())
        .map(|range| range.start)
}

#[cfg(test)]
mod tests {
    use super::{locate, module_declarations, predeclared};

    #[test]
    fn module_declarations_reads_every_kind_at_module_scope() {
        let source = "// fn commented\n@group(0) @binding(0) var<uniform> a: vec4<f32>;\nvar<private> b: f32;\nconst c = 1e-5;\noverride d: f32 = 0x1p+3f;\n/* struct e /* nested */ */ struct f { g: f32 };\nalias h = array<f32, 2>;\nfn i(j: f32) -> f32 {\n    var k = 1.0;\n    const l = 2.0;\n    return j + k + l;\n}\nconst_assert c > 0.0;\n";
        let names: Vec<_> = module_declarations(source)
            .into_iter()
            .map(|(at, name)| {
                assert!(source[at..].starts_with(name));
                name
            })
            .collect();
        assert_eq!(names, ["a", "b", "c", "d", "f", "h", "i"]);

        // naga's word rule: `_` or XID_Start, then XID_Continue, which
        // takes connector punctuation and combining marks.
        let names: Vec<_> = module_declarations(
            "const a\u{203f}fn = 1.0;\nfn min\u{301}() {}\nfn _x() {}\nconst \u{3b1}\u{3b2} = 1;\n",
        )
        .into_iter()
        .map(|(_, name)| name)
        .collect();
        assert_eq!(names, ["a\u{203f}fn", "min\u{301}", "_x", "\u{3b1}\u{3b2}"]);

        // naga's line breaks end a line comment; U+200E is blankspace.
        let names: Vec<_> = module_declarations(
            "// a\rfn b() {}\n// c\u{2028}alias d = f32;\n// e\u{85}const f = 1;\nfn\u{200e}g() {}\n/*/ fn h() {} */ fn i() {}\n",
        )
        .into_iter()
        .map(|(_, name)| name)
        .collect();
        assert_eq!(names, ["b", "d", "f", "g", "i"]);
    }

    #[test]
    fn module_names_reads_the_authors_declarations() {
        let prelude = "fn before() {}\n";
        let code = format!(
            "{prelude}fn f() {{}}\nvar<private> v: f32;\nconst c: f32 = 1.0;\nconst abstract_c = 1.0;\noverride o: f32 = 1.0;\nstruct S {{ a: f32 }};\nalias A = f32;\n"
        );
        let module = naga::front::wgsl::parse_str(&code).unwrap();
        let author = prelude.len()..code.len();
        let mut names: Vec<_> = super::module_names(&module, &author)
            .into_iter()
            .map(|(name, at)| {
                assert!(code[at..].contains(name), "{name}");
                name
            })
            .collect();
        names.sort_unstable();
        // naga keeps neither alias names nor abstract constants.
        assert_eq!(names, ["S", "c", "f", "o", "v"]);
    }

    #[test]
    fn predeclared_names_built_ins_and_types() {
        for name in [
            "clamp",
            "fwidth",
            "textureSample",
            "vec4f",
            "vec3",
            "mat3x3f",
            "mat2x4h",
            "f32",
            "texture_2d",
            "sampler",
            "array",
            "select",
        ] {
            assert!(predeclared(name), "{name}");
        }
        for name in [
            "texture_0",
            "sampler_0",
            "vec5",
            "mat3x5",
            "vec4data",
            "clamped",
            "params",
            "linear",
        ] {
            assert!(!predeclared(name), "{name}");
        }
    }

    #[test]
    fn locate_places_errors_by_where_they_lie() {
        let code = "head\nab\ncd\ntail";
        let author = 5..10;
        let at = |at| locate(code, &author, at, "oops".to_owned(), "(before)", "(after)");
        assert_eq!(at(Some(5)), "1:1: oops");
        assert_eq!(at(Some(9)), "2:2: oops");
        assert_eq!(at(Some(10)), "2:3: oops");
        assert_eq!(at(Some(2)), "oops (before)");
        assert_eq!(at(Some(12)), "oops (after)");
        assert_eq!(at(None), "oops");
        assert_eq!(
            locate(code, &author, Some(2), "oops".to_owned(), "", "(after)"),
            "oops"
        );
    }
}
