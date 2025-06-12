macro_rules! record_trace {
    ($($value:ident),+ $(,)?) => {
        if ::tracing::enabled!(::tracing::Level::TRACE) {
            let span = ::tracing::Span::current();
            $(
                span.record(stringify!($value), ::tracing::field::debug(&$value));
            )+
        }
    };
}

pub(crate) use record_trace;
