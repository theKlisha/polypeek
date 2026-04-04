//! [`Registry`] and [`Plugin`]: a directed graph of [`DataFormat`]
//! nodes and type-erased [`AnyTranscoder`] edges.
//!
//! Each [`Registry::add_codec`] call inserts or reuses nodes for the transcoder’s input and output
//! formats and adds one edge between them. [`Registry::path`] searches for a route from a source
//! format to a target format.

use itertools::Itertools;
use petgraph::graph::NodeIndex;
use petgraph::{Directed, Graph, algo};

use crate::transcoder::{AnyTranscoder, Data, DataFormat, Transcoder};

/// Extension hook to register a batch of codecs on a [`Registry`].
pub trait Plugin {
    /// Registers this plugin’s transcoders on `registry`.
    fn register(self, registry: &mut Registry);
}

/// Owns the conversion graph: formats as nodes, transcoders as edges.
#[derive(Default, Debug)]
pub struct Registry {
    graph: Graph<DataFormat, Box<dyn AnyTranscoder>, Directed>,
}

impl Registry {
    /// Registers all codecs provided by `plugin`.
    pub fn add_plugin(&mut self, plugin: impl Plugin) {
        plugin.register(self);
    }

    /// Registers a transcoder as an edge from `I::data_format()` to `O::data_format()`.
    pub fn add_codec<T, I, O>(&mut self, transcoder: T)
    where
        T: Transcoder<I, O>,
        I: Data,
        O: Data,
    {
        let t = transcoder.into_any();
        let dec = self.add_data_format(t.decodes());
        let enc = self.add_data_format(t.encodes());
        self.graph.add_edge(dec, enc, Box::new(t));
    }

    fn add_data_format(&mut self, data_format: DataFormat) -> NodeIndex {
        self.find_data_format(data_format)
            .unwrap_or_else(|| self.graph.add_node(data_format))
    }

    /// Returns the total cost and the sequence of transcoders along a lowest-cost path (by summed
    /// [`AnyTranscoder::planning_cost`] on each edge; A* with a zero heuristic) from `in_format` to
    /// `out_format`, or `None` if either format is missing or no path exists.
    pub fn path(
        &self,
        in_format: DataFormat,
        out_format: DataFormat,
    ) -> Option<(f32, Vec<&Box<dyn AnyTranscoder>>)> {
        let start = self.find_data_format(in_format)?;
        let target = self.find_data_format(out_format)?;
        let (cost, nodes) = algo::astar(
            &self.graph,
            start,
            |i| i == target,
            |e| e.weight().planning_cost(),
            |_| 0.0,
        )?;

        let codecs: Vec<_> = nodes
            .into_iter()
            .tuple_windows()
            .map(|(from, to)| {
                self.graph
                    .find_edge(from, to)
                    .expect("astar found the path yet there is no edge")
            })
            .map(|e| &self.graph[e])
            .collect();

        Some((cost, codecs))
    }

    fn find_data_format(&self, data_format: DataFormat) -> Option<NodeIndex> {
        self.graph
            .node_indices()
            .find(|i| self.graph[*i] == data_format)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcoder::{DataFormatMeta, Transcoder, TranscoderResult};

    #[derive(Debug, Default)]
    struct Fa;

    #[derive(Debug, Default)]
    struct Fb;

    #[derive(Debug, Default)]
    struct Fc;

    impl Data for Fa {
        fn data_format_meta() -> &'static DataFormatMeta {
            &DataFormatMeta {
                name: "fa",
                description: "",
            }
        }
    }

    impl Data for Fb {
        fn data_format_meta() -> &'static DataFormatMeta {
            &DataFormatMeta {
                name: "fb",
                description: "",
            }
        }
    }

    impl Data for Fc {
        fn data_format_meta() -> &'static DataFormatMeta {
            &DataFormatMeta {
                name: "fc",
                description: "",
            }
        }
    }

    struct AtoB;
    impl Transcoder<Fa, Fb> for AtoB {
        fn transcode(&self, _input: Fa) -> TranscoderResult<Fb> {
            Ok(Fb)
        }
        fn planning_cost(&self) -> f32 {
            1.0
        }
    }

    struct BtoC;
    impl Transcoder<Fb, Fc> for BtoC {
        fn transcode(&self, _input: Fb) -> TranscoderResult<Fc> {
            Ok(Fc)
        }
        fn planning_cost(&self) -> f32 {
            1.0
        }
    }

    /// Direct edge is expensive; two-hop path should win (total 2.0 vs 10.0).
    struct AtoC;
    impl Transcoder<Fa, Fc> for AtoC {
        fn transcode(&self, _input: Fa) -> TranscoderResult<Fc> {
            Ok(Fc)
        }
        fn planning_cost(&self) -> f32 {
            10.0
        }
    }

    #[test]
    fn path_prefers_lower_planning_cost_sum() {
        let mut registry = Registry::default();
        registry.add_codec(AtoB);
        registry.add_codec(BtoC);
        registry.add_codec(AtoC);

        let (total, chain) = registry
            .path(Fa::data_format(), Fc::data_format())
            .expect("path fa -> fc");

        assert!((total - 2.0).abs() < f32::EPSILON);
        assert_eq!(chain.len(), 2);
    }
}
