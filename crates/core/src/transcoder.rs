//! Typed payloads, format identity, and transcoder traits used by [`crate::registry::Registry`].
//!
//! A [`Data`] type describes one **lane** in the graph. [`Transcoder`] implements a single hop
//! `I → O`. Implementations can be structs or closures ([`Transcoder`] is implemented for
//! `Fn(I) -> TranscoderResult<O>`). [`AnyTranscoder`] type-erases a concrete transcoder so the
//! registry can store heterogeneous edges in one graph.

use std::{
    any::{Any, TypeId},
    fmt::Debug,
};

use thiserror::Error;

/// Stable identity for a [`Data`] type, backed by [`TypeId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DataFormatId(TypeId);

/// Human-readable labels for a format, used in errors and [`Debug`] output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DataFormatMeta {
    pub name: &'static str,
    pub description: &'static str,
}

impl DataFormatMeta {
    /// Placeholder when a type does not override [`Data::data_format_meta`].
    pub const EMPTY: Self = Self {
        name: "",
        description: "",
    };
}

/// Identity of a data format: the Rust type (`TypeId`) plus optional static metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DataFormat {
    type_id: DataFormatId,
    meta: &'static DataFormatMeta,
}

/// A concrete value type that can flow through transcoders and be registered as a graph node.
pub trait Data: Any + Sized {
    /// Static metadata for this format; defaults to [`DataFormatMeta::EMPTY`].
    fn data_format_meta() -> &'static DataFormatMeta {
        &DataFormatMeta::EMPTY
    }

    /// Format identity used as a node key in the registry graph.
    fn data_format() -> DataFormat {
        DataFormat {
            type_id: DataFormatId(TypeId::of::<Self>()),
            meta: Self::data_format_meta(),
        }
    }

    /// Wraps `self` in [`AnyData`] for type-erased transcoding.
    fn into_any(self) -> AnyData {
        AnyData {
            format: Self::data_format(),
            inner: Box::new(self),
        }
    }
}

/// A [`Data`] value boxed with its [`DataFormat`] for [`AnyTranscoder::transcode`].
pub struct AnyData {
    format: DataFormat,
    inner: Box<dyn Any>,
}

/// Errors from downcasting or from a transcoder implementation.
#[derive(Debug, Error)]
pub enum TranscoderError {
    #[error("codec type mismatch: expected {expected}, got {got}")]
    DataFormatMismatch {
        expected: &'static str,
        got: &'static str,
    },

    #[error("{0}")]
    Other(String),
}

/// Result type used by [`Transcoder::transcode`] and [`AnyTranscoder::transcode`].
pub type TranscoderResult<T> = Result<T, TranscoderError>;

/// One conversion step from input type `I` to output type `O`.
///
/// Closures and function pointers automatically implement this trait when their signature matches.
pub trait Transcoder<I: Data, O: Data>
where
    Self: Sized + 'static,
{
    /// Converts `input` to `O`, or returns an error.
    fn transcode(&self, input: I) -> TranscoderResult<O>;

    /// Positive weight used by [`crate::registry::Registry::path`] when no payload is available.
    /// Override this so cheaper codecs are preferred in multi-hop routes. Defaults to `1.0`.
    fn planning_cost(&self) -> f32 {
        1.0
    }

    /// Estimated cost of this hop for a concrete `input` (e.g. size-dependent work). Defaults to
    /// [`planning_cost`](Self::planning_cost); override for input-aware estimates.
    fn cost(&self, _input: I) -> f32 {
        self.planning_cost()
    }

    /// Wraps `self` in a type-erased [`AnyTranscoder`] for storage in the registry.
    fn into_any(self) -> impl AnyTranscoder {
        AnyTranscoderAdapter {
            inner: self,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, I, O> Transcoder<I, O> for T
where
    T: 'static,
    T: Fn(I) -> TranscoderResult<O>,
    I: Data,
    O: Data,
{
    fn transcode(&self, input: I) -> TranscoderResult<O> {
        self(input)
    }
}

/// Type-erased transcoder stored on each graph edge: decodes [`AnyData`] to `I`, runs the inner
/// codec, then re-boxes as `O`.
pub trait AnyTranscoder: Debug {
    /// Downcasts `input` to `I`, transcodes, then returns [`AnyData`] tagged with `O`.
    fn transcode(&self, input: AnyData) -> TranscoderResult<AnyData>;
    /// Typed cost using downcast input; see [`Transcoder::cost`].
    fn cost(&self, input: AnyData) -> TranscoderResult<f32>;
    /// Same meaning as [`Transcoder::planning_cost`]; used as the edge weight in [`crate::registry::Registry::path`].
    fn planning_cost(&self) -> f32;
    /// Input format `I` for this edge.
    fn decodes(&self) -> DataFormat;
    /// Output format `O` for this edge.
    fn encodes(&self) -> DataFormat;
}

struct AnyTranscoderAdapter<T, I, O> {
    inner: T,
    _marker: std::marker::PhantomData<(I, O)>,
}

impl<T, I, O> AnyTranscoder for AnyTranscoderAdapter<T, I, O>
where
    T: Transcoder<I, O>,
    I: Data,
    O: Data,
{
    fn transcode(&self, input: AnyData) -> TranscoderResult<AnyData> {
        let input = downcast_any_data(input)?;
        let output = self.inner.transcode(input)?;

        Ok(AnyData {
            format: O::data_format(),
            inner: Box::new(output),
        })
    }

    fn cost(&self, input: AnyData) -> TranscoderResult<f32> {
        let typed_input = downcast_any_data(input)?;
        Ok(self.inner.cost(typed_input))
    }

    fn planning_cost(&self) -> f32 {
        self.inner.planning_cost()
    }

    fn decodes(&self) -> DataFormat {
        I::data_format()
    }

    fn encodes(&self) -> DataFormat {
        O::data_format()
    }
}

impl<T, I, O> Debug for AnyTranscoderAdapter<T, I, O>
where
    T: Transcoder<I, O>,
    I: Data,
    O: Data,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&I::data_format().meta.name)?;
        f.write_str(" => ")?;
        f.write_str(&O::data_format().meta.name)
    }
}

fn downcast_any_data<T: Data>(input: AnyData) -> TranscoderResult<T> {
    input
        .inner
        .downcast::<T>()
        .map(|i| *i)
        .map_err(|actual| TranscoderError::DataFormatMismatch {
            expected: T::data_format().meta.name,
            got: input.format.meta.name,
        })
}
