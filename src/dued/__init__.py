"""dued: Python bindings for the Rust due-diligence engine."""

from dued.clones import find_clones, find_embed_clones, label_clusters
from dued.cost import cost_hint
from dued.dead import dead_report, dead_symbols
from dued.embed import DEFAULT_MODEL
from dued.fingerprints import fingerprint_overlap, fingerprint_symbol
from dued.graph import choose_call_targets
from dued.hollow import is_hollow
from dued.inventory import package_map
from dued.names import tokenize_name
from dued.parse import Extracted, Symbol, parse_source
from dued.rank import reading_order
from dued.risks import tag_risks
from dued.scan import run_scan
from dued.slice import slice_symbol
from dued.store import Index, connect
from dued.walk import SourceFile, walk_repo

__version__ = "0.1.0"

__all__ = [
    "DEFAULT_MODEL",
    "Extracted",
    "Index",
    "SourceFile",
    "Symbol",
    "__version__",
    "choose_call_targets",
    "connect",
    "cost_hint",
    "dead_report",
    "dead_symbols",
    "find_clones",
    "find_embed_clones",
    "fingerprint_overlap",
    "fingerprint_symbol",
    "is_hollow",
    "label_clusters",
    "package_map",
    "parse_source",
    "reading_order",
    "run_scan",
    "slice_symbol",
    "tag_risks",
    "tokenize_name",
    "walk_repo",
]
