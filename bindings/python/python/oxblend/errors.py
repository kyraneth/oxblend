"""Stable exact-code exceptions generated from the engine error catalog.

Regenerate with bindings/python/scripts/generate-errors.py. Never match messages.

Every class below documents its stable ``kind`` (see ``ERROR_TYPES``) and the
CLI catalog-help surface; ``help()`` on the installed package is the runtime
reference. Catalog presence does not imply terminal reachability: a kind may
only occur inside a report or in CLI-only/internal sites. The native base and
lifecycle exceptions receive runtime documentation in the package initializer.
"""
from __future__ import annotations
from ._native import OxblendError, ClosedFileError, SourceChangedError

class AmbiguousIdError(OxblendError):
    """More than one saved data-block matches the requested linked ID identity.

    Catalog kind ``ambiguous_id`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        AmbiguousIdError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error ambiguous_id`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class AmbiguousPointerError(OxblendError):
    """A saved address does not identify one unique target in the applicable namespace.

    Catalog kind ``ambiguous_pointer`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        AmbiguousPointerError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error ambiguous_pointer`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class BoundsError(OxblendError):
    """A selected block, record element or array index lies outside its checked extent.

    Catalog kind ``bounds`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        BoundsError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error bounds`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class OxblendBrokenPipeError(OxblendError):
    """The consumer closed stdout or another output pipe before Oxblend delivered everything.

    Catalog kind ``broken_pipe`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        OxblendBrokenPipeError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error broken_pipe`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ClosureOwnerError(OxblendError):
    """A graph operation cannot prove that every included direct-data allocation has an included owner.

    Catalog kind ``closure_owner`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ClosureOwnerError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error closure_owner`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class CompressionError(OxblendError):
    """The input container could not be decompressed as the expected blend byte stream.

    Catalog kind ``compression`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        CompressionError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error compression`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ConflictingEditsError(OxblendError):
    """Packing derived different replacement paths for the same saved owning field.

    Catalog kind ``conflicting_edits`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ConflictingEditsError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error conflicting_edits`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ConstructorTypeError(OxblendError):
    """The source file's DNA has no structure required by an internal graph constructor.

    Catalog kind ``constructor_type`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ConstructorTypeError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error constructor_type`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class CryptoRetargetError(OxblendError):
    """The planned Cryptomatte Scene-to-Image replacement cannot find its required imported records.

    Catalog kind ``crypto_retarget`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        CryptoRetargetError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error crypto_retarget`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class CryptoSceneKeptEdgeError(OxblendError):
    """A supposedly released Scene is still reachable through a kept graph edge.

    Catalog kind ``crypto_scene_kept_edge`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        CryptoSceneKeptEdgeError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error crypto_scene_kept_edge`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class CryptoSourceIdError(OxblendError):
    """The Cryptomatte node does not point to the Scene named by the conversion plan.

    Catalog kind ``crypto_source_id`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        CryptoSourceIdError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error crypto_source_id`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class CstringWidthError(OxblendError):
    """A graph name or string cannot be stored as a bounded NUL-terminated string in the selected field.

    Catalog kind ``cstring_width`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        CstringWidthError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error cstring_width`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class CycleError(OxblendError):
    """Traversal encountered a repeated active reference in a chain that must terminate.

    Catalog kind ``cycle`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        CycleError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error cycle`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class DanglingPointerError(OxblendError):
    """A nonzero saved pointer has no resolvable target in its checked namespace.

    Catalog kind ``dangling_pointer`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        DanglingPointerError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error dangling_pointer`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class DeclaredDiffError(OxblendError):
    """Staged typed-edit output differs outside the declared fixed-width changes or changed length.

    Catalog kind ``declared_diff`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        DeclaredDiffError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error declared_diff`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class DecodedProofError(OxblendError):
    """An internally emitted graph failed its decoded-byte readback check.

    Catalog kind ``decoded_proof`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        DecodedProofError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error decoded_proof`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class DiffExtentError(OxblendError):
    """A semantic comparison field would extend beyond its allocation.

    Catalog kind ``diff_extent`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        DiffExtentError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error diff_extent`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class DiffIdentityError(OxblendError):
    """The comparison could not assign unique stable identities to all required allocations.

    Catalog kind ``diff_identity`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        DiffIdentityError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error diff_identity`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class DiffPointerArrayError(OxblendError):
    """A pointer-array allocation's byte extent is not a multiple of pointer width.

    Catalog kind ``diff_pointer_array`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        DiffPointerArrayError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error diff_pointer_array`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class DuplicateDataAddressError(OxblendError):
    """The same native ID address occurs more than once in a namespace that requires uniqueness.

    Catalog kind ``duplicate_data_address`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        DuplicateDataAddressError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error duplicate_data_address`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class DurabilityError(OxblendError):
    """The requested persistence policy is invalid, unsupported here, or could not complete filesystem synchronization.

    Catalog kind ``durability`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        DurabilityError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error durability`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class EditConflictError(OxblendError):
    """Two edits overlap or are ordered incompatibly in the same decoded bytes.

    Catalog kind ``edit_conflict`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        EditConflictError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error edit_conflict`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class EditSpecError(OxblendError):
    """The edit/rewrite input is not the strict schema expected by that command.

    Catalog kind ``edit_spec`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        EditSpecError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error edit_spec`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class EditVersionError(OxblendError):
    """Typed editing is not admitted for this header version or endianness.

    Catalog kind ``edit_version`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        EditVersionError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error edit_version`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class EmptyClearError(OxblendError):
    """Path rewriting was asked to clear a reference rather than relocate it.

    Catalog kind ``empty_clear`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        EmptyClearError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error empty_clear`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class EncodingError(OxblendError):
    """A value required as text is not valid UTF-8.

    Catalog kind ``encoding`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        EncodingError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error encoding`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ExpectedOldError(OxblendError):
    """The exact saved old value or qualified pointer-clear site no longer matches the plan.

    Catalog kind ``expected_old`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ExpectedOldError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error expected_old`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ExpectedPathError(OxblendError):
    """The saved filepath differs byte-for-byte from the rewrite's expected old path.

    Catalog kind ``expected_path`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ExpectedPathError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error expected_path`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ExrDecodeError(OxblendError):
    """EXR pixel decoding could not complete under the selected saved structure and codec contract.

    Catalog kind ``exr_decode`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ExrDecodeError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error exr_decode`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ExrHeaderError(OxblendError):
    """The EXR header is malformed or incomplete for header parsing.

    Catalog kind ``exr_header`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ExrHeaderError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error exr_header`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ExrHeaderTruncatedError(OxblendError):
    """The available bytes end before a complete required EXR header or structural record can be read.

    Catalog kind ``exr_header_truncated`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ExrHeaderTruncatedError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error exr_header_truncated`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ExrSelectionError(OxblendError):
    """The requested EXR part or channel selection is absent or cannot be represented by the selected output contract.

    Catalog kind ``exr_selection`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ExrSelectionError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error exr_selection`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ExrUnsupportedError(OxblendError):
    """This EXR version, flag set or pass interpretation has no qualified support.

    Catalog kind ``exr_unsupported`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ExrUnsupportedError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error exr_unsupported`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ExtractRefusalError(OxblendError):
    """The selected partial file cannot be written under its explicit reference and pointer policies.

    Catalog kind ``extract_refusal`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ExtractRefusalError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error extract_refusal`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class FieldOwnerError(OxblendError):
    """The selected typed-edit field is not inside a qualified editable owner run.

    Catalog kind ``field_owner`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        FieldOwnerError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error field_owner`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class FieldTypeError(OxblendError):
    """A graph field path requires an embedded structure but the selected field is not one.

    Catalog kind ``field_type`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        FieldTypeError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error field_type`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class FileOpsProfileError(OxblendError):
    """Internal graph editing requires a qualified little-endian source layout.

    Catalog kind ``file_ops_profile`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        FileOpsProfileError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error file_ops_profile`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class FlagMaskError(OxblendError):
    """A masked integer edit sets and clears the same bit or uses bits outside the field width.

    Catalog kind ``flag_mask`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        FlagMaskError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error flag_mask`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IdentifierOverflowError(OxblendError):
    """A new node identifier cannot fit the required positive signed storage.

    Catalog kind ``identifier_overflow`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IdentifierOverflowError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error identifier_overflow`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IdpropStringError(OxblendError):
    """A guarded same-allocation IDProperty UTF-8 string value edit could not be proved.

    Catalog kind ``idprop_string`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IdpropStringError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error idprop_string`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IncompleteDiffError(OxblendError):
    """The comparison finished without enough evidence for an equality claim.

    Catalog kind ``incomplete_diff`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IncompleteDiffError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error incomplete_diff`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IncompleteEditError(OxblendError):
    """The edit report is incomplete; the specific cause is in its errors and publication state.

    Catalog kind ``incomplete_edit`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IncompleteEditError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error incomplete_edit`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IncompleteExrScanError(OxblendError):
    """The EXR scan retained independent observations but could not establish every requested member or qualification.

    Catalog kind ``incomplete_exr_scan`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IncompleteExrScanError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error incomplete_exr_scan`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IncompleteFactsError(OxblendError):
    """Independent saved facts were retained but one or more required queries failed.

    Catalog kind ``incomplete_facts`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IncompleteFactsError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error incomplete_facts`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IncompleteOverridesError(OxblendError):
    """The override report has unresolved or refused records and is not a complete inventory.

    Catalog kind ``incomplete_overrides`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IncompleteOverridesError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error incomplete_overrides`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IncompletePackError(OxblendError):
    """Packing could not deliver a complete selected dependency closure.

    Catalog kind ``incomplete_pack`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IncompletePackError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error incomplete_pack`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IncompleteQueryError(OxblendError):
    """The requested query contains unavailable field outcomes; independent saved rows and fields remain in stdout.

    Catalog kind ``incomplete_query`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IncompleteQueryError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error incomplete_query`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IncompleteReadBatchError(OxblendError):
    """The read batch retained independent item results but one or more planned reads failed; the terminal summary is incomplete.

    Catalog kind ``incomplete_read_batch`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IncompleteReadBatchError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error incomplete_read_batch`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IncompleteTraceError(OxblendError):
    """Dependency traversal was not trustworthy enough to call complete; list may therefore withhold its listing.

    Catalog kind ``incomplete_trace`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IncompleteTraceError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error incomplete_trace`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class InvalidBlockError(OxblendError):
    """A saved blend block violates the container's structural rules.

    Catalog kind ``invalid_block`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        InvalidBlockError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error invalid_block`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class InvalidGraphError(OxblendError):
    """A dependency graph relationship violates the reader's required structure.

    Catalog kind ``invalid_graph`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        InvalidGraphError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error invalid_graph`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class InvalidHeaderError(OxblendError):
    """The decoded input does not contain a valid supported Blender header.

    Catalog kind ``invalid_header`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        InvalidHeaderError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error invalid_header`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class InvalidLayoutError(OxblendError):
    """A saved record does not match the layout required for the operation.

    Catalog kind ``invalid_layout`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        InvalidLayoutError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error invalid_layout`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class InvalidSavedStructureError(OxblendError):
    """A requested saved projection lacks a required structural relationship, so its meaning cannot be certified.

    Catalog kind ``invalid_saved_structure`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        InvalidSavedStructureError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error invalid_saved_structure`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class InvalidSdnaError(OxblendError):
    """The saved DNA schema is structurally inconsistent or cannot be interpreted safely.

    Catalog kind ``invalid_sdna`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        InvalidSdnaError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error invalid_sdna`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class IoError(OxblendError):
    """An operating-system read, write, metadata or resource operation failed.

    Catalog kind ``io`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        IoError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error io`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class JsonError(OxblendError):
    """A library-facing JSON report could not be encoded.

    Catalog kind ``json`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        JsonError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error json`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class KitDnaError(OxblendError):
    """A constructor/import kit does not match the destination's DNA and container layout.

    Catalog kind ``kit_dna`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        KitDnaError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error kit_dna`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class LibraryReadError(OxblendError):
    """Trace could not open or read a referenced blend library.

    Catalog kind ``library_read`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        LibraryReadError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error library_read`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class LibraryReaderError(OxblendError):
    """Override traversal could not read a referenced library and retains its underlying reader kind.

    Catalog kind ``library_reader`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        LibraryReaderError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error library_reader`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class LinkOwnerError(OxblendError):
    """A node link or its endpoints do not belong to the required node tree/directions.

    Catalog kind ``link_owner`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        LinkOwnerError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error link_owner`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class LinkProfileError(OxblendError):
    """The requested node-link operation is not qualified for this socket arrangement.

    Catalog kind ``link_profile`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        LinkProfileError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error link_profile`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class LinkedReferenceOnlyError(OxblendError):
    """The selected root-file ID is a linked reference, not an available local body.

    Catalog kind ``linked_reference_only`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        LinkedReferenceOnlyError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error linked_reference_only`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ListBacklinkError(OxblendError):
    """A saved intrusive-list element's prev pointer is not its actual predecessor.

    Catalog kind ``list_backlink`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ListBacklinkError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error list_backlink`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ListBoundaryError(OxblendError):
    """An intrusive list's saved first/last boundaries disagree with its chain.

    Catalog kind ``list_boundary`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ListBoundaryError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error list_boundary`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ListCycleError(OxblendError):
    """A saved intrusive next-chain loops instead of terminating.

    Catalog kind ``list_cycle`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ListCycleError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error list_cycle`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ListEndpointError(OxblendError):
    """An internal graph list's walked final element differs from its saved last endpoint.

    Catalog kind ``list_endpoint`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ListEndpointError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error list_endpoint`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ListEndpointsError(OxblendError):
    """A declared intrusive list lacks consistent nonnull endpoints or ends at a different last element.

    Catalog kind ``list_endpoints`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ListEndpointsError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error list_endpoints`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ListHeaderError(OxblendError):
    """A list element lacks the required next/prev intrusive header at its expected offsets.

    Catalog kind ``list_header`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ListHeaderError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error list_header`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ListIntegrityError(OxblendError):
    """A source-list walk failed its combined cycle, symmetry or endpoint checks.

    Catalog kind ``list_integrity`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ListIntegrityError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error list_integrity`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ListReciprocityError(OxblendError):
    """A declared intrusive list's backward edge does not reciprocate the walked next edge.

    Catalog kind ``list_reciprocity`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ListReciprocityError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error list_reciprocity`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ListSymmetryError(OxblendError):
    """A graph list element's prev edge disagrees with its predecessor.

    Catalog kind ``list_symmetry`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ListSymmetryError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error list_symmetry`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class MissingAddressError(OxblendError):
    """The requested diagnostic address is null or absent in this exact input.

    Catalog kind ``missing_address`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        MissingAddressError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error missing_address`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class MissingAssetError(OxblendError):
    """A declared external asset has no available required file/member at its resolved location.

    Catalog kind ``missing_asset`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        MissingAssetError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error missing_asset`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class MissingFieldError(OxblendError):
    """The selected saved structure has no field with the requested name.

    Catalog kind ``missing_field`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        MissingFieldError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error missing_field`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class MissingIdError(OxblendError):
    """The library file exists but does not contain the linked data-block requested by saved ID code/name.

    Catalog kind ``missing_id`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        MissingIdError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error missing_id`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class MissingInputError(OxblendError):
    """A selected input blend file is missing or not an eligible file.

    Catalog kind ``missing_input`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        MissingInputError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error missing_input`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class MissingLibraryError(OxblendError):
    """A referenced external blend library cannot be found at its resolved location.

    Catalog kind ``missing_library`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        MissingLibraryError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error missing_library`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class NameCollisionError(OxblendError):
    """A new graph node/socket name is already used where uniqueness is required.

    Catalog kind ``name_collision`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        NameCollisionError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error name_collision`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class NodeConstructorError(OxblendError):
    """No qualified native prototype is available for the requested node kind.

    Catalog kind ``node_constructor`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        NodeConstructorError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error node_constructor`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class NodeCycleError(OxblendError):
    """The saved compositor graph contains a cycle unsupported by this structural profile.

    Catalog kind ``node_cycle`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        NodeCycleError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error node_cycle`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class NodeEndpointError(OxblendError):
    """A node, socket or link endpoint does not resolve to exactly one valid tree member.

    Catalog kind ``node_endpoint`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        NodeEndpointError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error node_endpoint`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class NodeExpectedGraphError(OxblendError):
    """The saved graph or node type differs from the explicitly expected graph.

    Catalog kind ``node_expected_graph`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        NodeExpectedGraphError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error node_expected_graph`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class NodeGraphError(OxblendError):
    """The saved compositor structure is outside the supported checked graph profile.

    Catalog kind ``node_graph`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        NodeGraphError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error node_graph`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class NodeOccupiedError(OxblendError):
    """The target input already has a guarded existing link.

    Catalog kind ``node_occupied`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        NodeOccupiedError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error node_occupied`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class NodeOwnerError(OxblendError):
    """The requested node is not a member of the selected tree.

    Catalog kind ``node_owner`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        NodeOwnerError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error node_owner`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class NodeSocketTypeError(OxblendError):
    """Endpoint socket types differ from their exact expected types or from each other.

    Catalog kind ``node_socket_type`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        NodeSocketTypeError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error node_socket_type`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class NodeVersionError(OxblendError):
    """The input does not match the supported native Blender structural profile.

    Catalog kind ``node_version`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        NodeVersionError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error node_version`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class OutputCompressionError(OxblendError):
    """The staged compressed output failed its decoder verification.

    Catalog kind ``output_compression`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        OutputCompressionError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error output_compression`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class OutputDecodeLengthError(OxblendError):
    """Decoding the staged output produced a length different from the immutable decoded source body.

    Catalog kind ``output_decode_length`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        OutputDecodeLengthError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error output_decode_length`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class OutputExistsError(OxblendError):
    """A no-replace output path already exists or appeared during publication.

    Catalog kind ``output_exists`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        OutputExistsError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error output_exists`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class OutputPathError(OxblendError):
    """The chosen output filename/parent is invalid or reserved for staging.

    Catalog kind ``output_path`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        OutputPathError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error output_path`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class OxblendOverflowError(OxblendError):
    """A checked length, offset, count or address calculation exceeds its representable width.

    Catalog kind ``overflow`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        OxblendOverflowError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error overflow`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class OwnerHeaderError(OxblendError):
    """A block cannot be qualified as the native ID/USER owner its header claims.

    Catalog kind ``owner_header`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        OwnerHeaderError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error owner_header`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class OwnerProjectionError(OxblendError):
    """The emitted graph's re-read owner mapping differs from its planned owner mapping.

    Catalog kind ``owner_projection`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        OwnerProjectionError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error owner_projection`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PackCollisionError(OxblendError):
    """Different planned members or generated pack files occupy the same final path.

    Catalog kind ``pack_collision`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PackCollisionError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pack_collision`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PackDestinationError(OxblendError):
    """The destination is not eligible for a safe new pack.

    Catalog kind ``pack_destination`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PackDestinationError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pack_destination`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PackInputError(OxblendError):
    """The selected pack/list/blocks input does not exist as an eligible file.

    Catalog kind ``pack_input`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PackInputError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pack_input`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PackOriginError(OxblendError):
    """A traced path cannot be matched to its checked saved owner and field for relocation.

    Catalog kind ``pack_origin`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PackOriginError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pack_origin`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PackPathError(OxblendError):
    """A pack source or destination path violates the safe path policy.

    Catalog kind ``pack_path`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PackPathError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pack_path`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PackProjectError(OxblendError):
    """The explicit project directory cannot be selected because it does not exist.

    Catalog kind ``pack_project`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PackProjectError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pack_project`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PaddingWriteError(OxblendError):
    """A typed edit targets serialized padding rather than an admitted semantic leaf.

    Catalog kind ``padding_write`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PaddingWriteError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error padding_write`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PathError(OxblendError):
    """The field-selection path is malformed or cannot be traversed as written.

    Catalog kind ``path`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PathError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error path`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PathContextError(OxblendError):
    """Path rewriting cannot safely establish the required contextual import-input list.

    Catalog kind ``path_context`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PathContextError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error path_context`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PathNulError(OxblendError):
    """A rewrite path contains an embedded NUL byte.

    Catalog kind ``path_nul`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PathNulError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error path_nul`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PathOverflowError(OxblendError):
    """The replacement path plus its terminator does not fit the fixed saved field.

    Catalog kind ``path_overflow`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PathOverflowError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error path_overflow`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PathResolutionUnverifiedError(OxblendError):
    """The saved path spelling is known but its resolution has not been verified.

    Catalog kind ``path_resolution_unverified`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PathResolutionUnverifiedError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error path_resolution_unverified`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PathTerminationError(OxblendError):
    """The old fixed-width path has no NUL terminator inside its saved capacity.

    Catalog kind ``path_termination`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PathTerminationError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error path_termination`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PatternMembersUnknownError(OxblendError):
    """The saved image pattern is known, but its complete required member set could not be proved.

    Catalog kind ``pattern_members_unknown`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PatternMembersUnknownError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pattern_members_unknown`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PointerCountError(OxblendError):
    """The pointed record count does not satisfy its role's required count.

    Catalog kind ``pointer_count`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PointerCountError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pointer_count`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PointerDepthError(OxblendError):
    """A pointer-table entry was requested from a field that is not pointer-to-pointer.

    Catalog kind ``pointer_depth`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PointerDepthError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pointer_depth`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PointerExtentError(OxblendError):
    """A pointer target's allocation size/count/length does not match its qualified role.

    Catalog kind ``pointer_extent`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PointerExtentError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pointer_extent`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PointerPathError(OxblendError):
    """A field path attempts to walk through a pointer implicitly.

    Catalog kind ``pointer_path`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PointerPathError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pointer_path`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PointerProofError(OxblendError):
    """An emitted graph still has unresolved or ambiguous pointers on re-read.

    Catalog kind ``pointer_proof`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PointerProofError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pointer_proof`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PointerRoleError(OxblendError):
    """The pointer's declared owner/field/discriminator cannot be interpreted as the required role.

    Catalog kind ``pointer_role`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PointerRoleError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pointer_role`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PointerTargetTypeError(OxblendError):
    """A pointer resolved, but its target is not the type/storage family required by the owning role.

    Catalog kind ``pointer_target_type`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PointerTargetTypeError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pointer_target_type`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PointerWriteError(OxblendError):
    """The requested operation would write a pointer or pointer-backed path outside admitted repairs.

    Catalog kind ``pointer_write`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PointerWriteError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error pointer_write`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ProfileDiscriminatorError(OxblendError):
    """The requested compositing edit does not match the qualified reader/Image discriminator.

    Catalog kind ``profile_discriminator`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ProfileDiscriminatorError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error profile_discriminator`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ProfileFieldError(OxblendError):
    """The field/value/index selection has no admitted compositing profile.

    Catalog kind ``profile_field`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ProfileFieldError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error profile_field`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ProfileLibraryError(OxblendError):
    """Library retargeting cannot prove an admitted equivalent-content relocation.

    Catalog kind ``profile_library`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ProfileLibraryError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error profile_library`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ProfileLinkedError(OxblendError):
    """The compositing target is linked or override-protected rather than an editable local owner.

    Catalog kind ``profile_linked`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ProfileLinkedError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error profile_linked`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ProfileMaskError(OxblendError):
    """Masked flag writes are not admitted in compositing-profile mode.

    Catalog kind ``profile_mask`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ProfileMaskError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error profile_mask`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ProfileOwnerError(OxblendError):
    """The comp edit is not owned by a qualified local Scene, Image or node owner.

    Catalog kind ``profile_owner`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ProfileOwnerError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error profile_owner`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ProfileTemporalError(OxblendError):
    """The final edited Scene/reader timing is inconsistent with the qualified temporal profile.

    Catalog kind ``profile_temporal`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ProfileTemporalError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error profile_temporal`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PropertyCollisionError(OxblendError):
    """A custom-property group already contains the proposed property name.

    Catalog kind ``property_collision`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PropertyCollisionError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error property_collision`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PropertyOwnerError(OxblendError):
    """A custom property is not detached and owner-local to its intended group.

    Catalog kind ``property_owner`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PropertyOwnerError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error property_owner`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PropertySelectionError(OxblendError):
    """The custom-property operation did not select exactly one property.

    Catalog kind ``property_selection`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PropertySelectionError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error property_selection`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class PropertyTypeError(OxblendError):
    """A custom-property group operation received a nongroup IDProperty.

    Catalog kind ``property_type`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        PropertyTypeError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error property_type`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class QueryCycleError(OxblendError):
    """A saved query structure revisits an identity where an acyclic list or tree is required.

    Catalog kind ``query_cycle`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        QueryCycleError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error query_cycle`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class QueryListError(OxblendError):
    """A saved list's previous/next/end controls disagree, so its complete ordered content cannot be certified.

    Catalog kind ``query_list`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        QueryListError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error query_list`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class RawProfileError(OxblendError):
    """Raw-allocation admission was requested for a typed rather than opaque allocation.

    Catalog kind ``raw_profile`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        RawProfileError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error raw_profile`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class RecordExtentError(OxblendError):
    """A typed record's count and per-record size do not equal its allocation extent.

    Catalog kind ``record_extent`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        RecordExtentError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error record_extent`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class RequiredPointerNullError(OxblendError):
    """A pointer required by its qualified role is NULL.

    Catalog kind ``required_pointer_null`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        RequiredPointerNullError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error required_pointer_null`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class RetainedRemovedError(OxblendError):
    """A graph operation removes a record still referenced by a kept record.

    Catalog kind ``retained_removed`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        RetainedRemovedError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error retained_removed`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class RlayersSourceIdError(OxblendError):
    """A planned Render Layers source edge is not actually a Scene reference.

    Catalog kind ``rlayers_source_id`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        RlayersSourceIdError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error rlayers_source_id`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class SameInputError(OxblendError):
    """The output aliases the original input, so no-replace editing is refused.

    Catalog kind ``same_input`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        SameInputError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error same_input`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class SeekTableError(OxblendError):
    """A compressed seek-table/framing proof is absent or inconsistent.

    Catalog kind ``seek_table`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        SeekTableError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error seek_table`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class SerializationError(OxblendError):
    """The typed-edit report could not be serialized for stdout.

    Catalog kind ``serialization`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        SerializationError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error serialization`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class Sha256WidthError(OxblendError):
    """The diagnostic SHA-256 length accounting exceeded its representable message width.

    Catalog kind ``sha256_width`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        Sha256WidthError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error sha256_width`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class SnapshotMismatchError(OxblendError):
    """A field reference or edit plan belongs to a different immutable snapshot.

    Catalog kind ``snapshot_mismatch`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        SnapshotMismatchError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error snapshot_mismatch`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class SocketConstructorError(OxblendError):
    """No qualified native socket/default/pass-storage construction matches this request.

    Catalog kind ``socket_constructor`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        SocketConstructorError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error socket_constructor`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class SocketOwnerError(OxblendError):
    """A socket is not owned by the required node and direction.

    Catalog kind ``socket_owner`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        SocketOwnerError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error socket_owner`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class OxblendSourceChangedError(OxblendError):
    """The live read backing changed identity or content during a scan.

    Catalog kind ``source_changed`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        OxblendSourceChangedError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error source_changed`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class SpillSpaceError(OxblendError):
    """There is not enough available temporary filesystem space for the required decoded spill.

    Catalog kind ``spill_space`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        SpillSpaceError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error spill_space`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class StorageProfileError(OxblendError):
    """A node storage layout lacks the schema facts required by its qualified graph profile.

    Catalog kind ``storage_profile`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        StorageProfileError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error storage_profile`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class StrayDataRunError(OxblendError):
    """An owner-local DATA block appears outside a qualified ID/USER run.

    Catalog kind ``stray_data_run`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        StrayDataRunError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error stray_data_run`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class StringNulError(OxblendError):
    """A typed CString replacement contains an embedded NUL.

    Catalog kind ``string_nul`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        StringNulError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error string_nul`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class StringTerminationError(OxblendError):
    """An old or planned fixed char string has no bounded terminator.

    Catalog kind ``string_termination`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        StringTerminationError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error string_termination`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class TextConstructorError(OxblendError):
    """The constructor kit has no qualified Text prototype to import.

    Catalog kind ``text_constructor`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        TextConstructorError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error text_constructor`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class TruncatedError(OxblendError):
    """The input or backing data ends before a required header, field, payload or copy extent.

    Catalog kind ``truncated`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        TruncatedError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error truncated`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class OxblendTypeError(OxblendError):
    """The selected value is not the kind the operation expects.

    Catalog kind ``type`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        OxblendTypeError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error type`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class TypedKindError(OxblendError):
    """A typed-edit value tag does not exactly match the saved primitive or whole-array kind.

    Catalog kind ``typed_kind`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        TypedKindError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error typed_kind`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class TypedOverflowError(OxblendError):
    """A typed replacement cannot fit the saved integer or string capacity.

    Catalog kind ``typed_overflow`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        TypedOverflowError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error typed_overflow`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class TypedWidthError(OxblendError):
    """The declared type/width/array shape does not exactly match saved storage.

    Catalog kind ``typed_width`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        TypedWidthError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error typed_width`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class UnknownConceptError(OxblendError):
    """The exact name is neither an authored catalog concept nor an actual type in the selected file's SDNA.

    Catalog kind ``unknown_concept`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        UnknownConceptError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error unknown_concept`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class UnprofiledRoleError(OxblendError):
    """Oxblend has no qualified interpretation for this saved pointer/list/storage role or discriminator.

    Catalog kind ``unprofiled_role`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        UnprofiledRoleError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error unprofiled_role`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class UnresolvedPersistentError(OxblendError):
    """A persistent saved pointer has no target in the required namespace/owner context.

    Catalog kind ``unresolved_persistent`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        UnresolvedPersistentError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error unresolved_persistent`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class UnsupportedEditError(OxblendError):
    """Path rewrite is not an arbitrary setter and this target is outside its whitelist.

    Catalog kind ``unsupported_edit`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        UnsupportedEditError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error unsupported_edit`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class UnsupportedFormatError(OxblendError):
    """The container/header variant is not a supported Blender serialization format.

    Catalog kind ``unsupported_format`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        UnsupportedFormatError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error unsupported_format`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class UnsupportedTypeError(OxblendError):
    """The selected record/field is outside this operation's supported typed interpretation.

    Catalog kind ``unsupported_type`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        UnsupportedTypeError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error unsupported_type`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class UnverifiedSemanticsError(OxblendError):
    """The requested saved-fact semantic projection has not been qualified for this representation.

    Catalog kind ``unverified_semantics`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        UnverifiedSemanticsError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error unverified_semantics`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class UsageError(OxblendError):
    """The command arguments, option values or argument count do not match the CLI contract.

    Catalog kind ``usage`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        UsageError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error usage`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ZipEncodingError(OxblendError):
    """A ZIP member name cannot be represented in the writer's required UTF-8 encoding.

    Catalog kind ``zip_encoding`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ZipEncodingError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error zip_encoding`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ZipPathError(OxblendError):
    """A ZIP member would escape the workspace or has unsafe path components.

    Catalog kind ``zip_path`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ZipPathError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error zip_path`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

class ZipWidthError(OxblendError):
    """A ZIP member name exceeds the format field used by this writer.

    Catalog kind ``zip_width`` maps to this exception class in ERROR_TYPES.
    Some catalog kinds are report-only, CLI-only or internal: this mapping
    does not promise that a Python operation can raise every class.
    Engine-emitted errors retain canonical fields and any partial report;
    matching this class or error.kind is safer than matching message text.

    Args:
        *args: Ordinary Python Exception arguments for manual construction.
            Constructing an exception does not execute the engine, assign
            canonical fields, or prove that this failure occurred.

    Returns:
        ZipWidthError: An exception instance retaining its positional args.

    Raises:
        TypeError: Unsupported keyword arguments to the inherited constructor.

    CLI: ``oxblend help error zip_width`` describes this exact catalog kind
    and its likely causes/remedies; this is not an operation invocation.
    """

ERROR_TYPES: dict[str, type[OxblendError]] = {
    'ambiguous_id': AmbiguousIdError,
    'ambiguous_pointer': AmbiguousPointerError,
    'bounds': BoundsError,
    'broken_pipe': OxblendBrokenPipeError,
    'closure_owner': ClosureOwnerError,
    'compression': CompressionError,
    'conflicting_edits': ConflictingEditsError,
    'constructor_type': ConstructorTypeError,
    'crypto_retarget': CryptoRetargetError,
    'crypto_scene_kept_edge': CryptoSceneKeptEdgeError,
    'crypto_source_id': CryptoSourceIdError,
    'cstring_width': CstringWidthError,
    'cycle': CycleError,
    'dangling_pointer': DanglingPointerError,
    'declared_diff': DeclaredDiffError,
    'decoded_proof': DecodedProofError,
    'diff_extent': DiffExtentError,
    'diff_identity': DiffIdentityError,
    'diff_pointer_array': DiffPointerArrayError,
    'duplicate_data_address': DuplicateDataAddressError,
    'durability': DurabilityError,
    'edit_conflict': EditConflictError,
    'edit_spec': EditSpecError,
    'edit_version': EditVersionError,
    'empty_clear': EmptyClearError,
    'encoding': EncodingError,
    'expected_old': ExpectedOldError,
    'expected_path': ExpectedPathError,
    'exr_decode': ExrDecodeError,
    'exr_header': ExrHeaderError,
    'exr_header_truncated': ExrHeaderTruncatedError,
    'exr_selection': ExrSelectionError,
    'exr_unsupported': ExrUnsupportedError,
    'extract_refusal': ExtractRefusalError,
    'field_owner': FieldOwnerError,
    'field_type': FieldTypeError,
    'file_ops_profile': FileOpsProfileError,
    'flag_mask': FlagMaskError,
    'identifier_overflow': IdentifierOverflowError,
    'idprop_string': IdpropStringError,
    'incomplete_diff': IncompleteDiffError,
    'incomplete_edit': IncompleteEditError,
    'incomplete_exr_scan': IncompleteExrScanError,
    'incomplete_facts': IncompleteFactsError,
    'incomplete_overrides': IncompleteOverridesError,
    'incomplete_pack': IncompletePackError,
    'incomplete_query': IncompleteQueryError,
    'incomplete_read_batch': IncompleteReadBatchError,
    'incomplete_trace': IncompleteTraceError,
    'input_changed': SourceChangedError,
    'invalid_block': InvalidBlockError,
    'invalid_graph': InvalidGraphError,
    'invalid_header': InvalidHeaderError,
    'invalid_layout': InvalidLayoutError,
    'invalid_saved_structure': InvalidSavedStructureError,
    'invalid_sdna': InvalidSdnaError,
    'io': IoError,
    'json': JsonError,
    'kit_dna': KitDnaError,
    'library_read': LibraryReadError,
    'library_reader': LibraryReaderError,
    'link_owner': LinkOwnerError,
    'link_profile': LinkProfileError,
    'linked_reference_only': LinkedReferenceOnlyError,
    'list_backlink': ListBacklinkError,
    'list_boundary': ListBoundaryError,
    'list_cycle': ListCycleError,
    'list_endpoint': ListEndpointError,
    'list_endpoints': ListEndpointsError,
    'list_header': ListHeaderError,
    'list_integrity': ListIntegrityError,
    'list_reciprocity': ListReciprocityError,
    'list_symmetry': ListSymmetryError,
    'missing_address': MissingAddressError,
    'missing_asset': MissingAssetError,
    'missing_field': MissingFieldError,
    'missing_id': MissingIdError,
    'missing_input': MissingInputError,
    'missing_library': MissingLibraryError,
    'name_collision': NameCollisionError,
    'node_constructor': NodeConstructorError,
    'node_cycle': NodeCycleError,
    'node_endpoint': NodeEndpointError,
    'node_expected_graph': NodeExpectedGraphError,
    'node_graph': NodeGraphError,
    'node_occupied': NodeOccupiedError,
    'node_owner': NodeOwnerError,
    'node_socket_type': NodeSocketTypeError,
    'node_version': NodeVersionError,
    'output_compression': OutputCompressionError,
    'output_decode_length': OutputDecodeLengthError,
    'output_exists': OutputExistsError,
    'output_path': OutputPathError,
    'overflow': OxblendOverflowError,
    'owner_header': OwnerHeaderError,
    'owner_projection': OwnerProjectionError,
    'pack_collision': PackCollisionError,
    'pack_destination': PackDestinationError,
    'pack_input': PackInputError,
    'pack_origin': PackOriginError,
    'pack_path': PackPathError,
    'pack_project': PackProjectError,
    'padding_write': PaddingWriteError,
    'path': PathError,
    'path_context': PathContextError,
    'path_nul': PathNulError,
    'path_overflow': PathOverflowError,
    'path_resolution_unverified': PathResolutionUnverifiedError,
    'path_termination': PathTerminationError,
    'pattern_members_unknown': PatternMembersUnknownError,
    'pointer_count': PointerCountError,
    'pointer_depth': PointerDepthError,
    'pointer_extent': PointerExtentError,
    'pointer_path': PointerPathError,
    'pointer_proof': PointerProofError,
    'pointer_role': PointerRoleError,
    'pointer_target_type': PointerTargetTypeError,
    'pointer_write': PointerWriteError,
    'profile_discriminator': ProfileDiscriminatorError,
    'profile_field': ProfileFieldError,
    'profile_library': ProfileLibraryError,
    'profile_linked': ProfileLinkedError,
    'profile_mask': ProfileMaskError,
    'profile_owner': ProfileOwnerError,
    'profile_temporal': ProfileTemporalError,
    'property_collision': PropertyCollisionError,
    'property_owner': PropertyOwnerError,
    'property_selection': PropertySelectionError,
    'property_type': PropertyTypeError,
    'query_cycle': QueryCycleError,
    'query_list': QueryListError,
    'raw_profile': RawProfileError,
    'record_extent': RecordExtentError,
    'required_pointer_null': RequiredPointerNullError,
    'retained_removed': RetainedRemovedError,
    'rlayers_source_id': RlayersSourceIdError,
    'same_input': SameInputError,
    'seek_table': SeekTableError,
    'serialization': SerializationError,
    'sha256_width': Sha256WidthError,
    'snapshot_mismatch': SnapshotMismatchError,
    'socket_constructor': SocketConstructorError,
    'socket_owner': SocketOwnerError,
    'source_changed': OxblendSourceChangedError,
    'spill_space': SpillSpaceError,
    'storage_profile': StorageProfileError,
    'stray_data_run': StrayDataRunError,
    'string_nul': StringNulError,
    'string_termination': StringTerminationError,
    'text_constructor': TextConstructorError,
    'truncated': TruncatedError,
    'type': OxblendTypeError,
    'typed_kind': TypedKindError,
    'typed_overflow': TypedOverflowError,
    'typed_width': TypedWidthError,
    'unknown_concept': UnknownConceptError,
    'unprofiled_role': UnprofiledRoleError,
    'unresolved_persistent': UnresolvedPersistentError,
    'unsupported_edit': UnsupportedEditError,
    'unsupported_format': UnsupportedFormatError,
    'unsupported_type': UnsupportedTypeError,
    'unverified_semantics': UnverifiedSemanticsError,
    'usage': UsageError,
    'zip_encoding': ZipEncodingError,
    'zip_path': ZipPathError,
    'zip_width': ZipWidthError,
    'closed_file': ClosedFileError,
}

__all__ = ["ERROR_TYPES", "OxblendError", "ClosedFileError", "SourceChangedError", 'AmbiguousIdError', 'AmbiguousPointerError', 'BoundsError', 'OxblendBrokenPipeError', 'ClosureOwnerError', 'CompressionError', 'ConflictingEditsError', 'ConstructorTypeError', 'CryptoRetargetError', 'CryptoSceneKeptEdgeError', 'CryptoSourceIdError', 'CstringWidthError', 'CycleError', 'DanglingPointerError', 'DeclaredDiffError', 'DecodedProofError', 'DiffExtentError', 'DiffIdentityError', 'DiffPointerArrayError', 'DuplicateDataAddressError', 'DurabilityError', 'EditConflictError', 'EditSpecError', 'EditVersionError', 'EmptyClearError', 'EncodingError', 'ExpectedOldError', 'ExpectedPathError', 'ExrDecodeError', 'ExrHeaderError', 'ExrHeaderTruncatedError', 'ExrSelectionError', 'ExrUnsupportedError', 'ExtractRefusalError', 'FieldOwnerError', 'FieldTypeError', 'FileOpsProfileError', 'FlagMaskError', 'IdentifierOverflowError', 'IdpropStringError', 'IncompleteDiffError', 'IncompleteEditError', 'IncompleteExrScanError', 'IncompleteFactsError', 'IncompleteOverridesError', 'IncompletePackError', 'IncompleteQueryError', 'IncompleteReadBatchError', 'IncompleteTraceError', 'InvalidBlockError', 'InvalidGraphError', 'InvalidHeaderError', 'InvalidLayoutError', 'InvalidSavedStructureError', 'InvalidSdnaError', 'IoError', 'JsonError', 'KitDnaError', 'LibraryReadError', 'LibraryReaderError', 'LinkOwnerError', 'LinkProfileError', 'LinkedReferenceOnlyError', 'ListBacklinkError', 'ListBoundaryError', 'ListCycleError', 'ListEndpointError', 'ListEndpointsError', 'ListHeaderError', 'ListIntegrityError', 'ListReciprocityError', 'ListSymmetryError', 'MissingAddressError', 'MissingAssetError', 'MissingFieldError', 'MissingIdError', 'MissingInputError', 'MissingLibraryError', 'NameCollisionError', 'NodeConstructorError', 'NodeCycleError', 'NodeEndpointError', 'NodeExpectedGraphError', 'NodeGraphError', 'NodeOccupiedError', 'NodeOwnerError', 'NodeSocketTypeError', 'NodeVersionError', 'OutputCompressionError', 'OutputDecodeLengthError', 'OutputExistsError', 'OutputPathError', 'OxblendOverflowError', 'OwnerHeaderError', 'OwnerProjectionError', 'PackCollisionError', 'PackDestinationError', 'PackInputError', 'PackOriginError', 'PackPathError', 'PackProjectError', 'PaddingWriteError', 'PathError', 'PathContextError', 'PathNulError', 'PathOverflowError', 'PathResolutionUnverifiedError', 'PathTerminationError', 'PatternMembersUnknownError', 'PointerCountError', 'PointerDepthError', 'PointerExtentError', 'PointerPathError', 'PointerProofError', 'PointerRoleError', 'PointerTargetTypeError', 'PointerWriteError', 'ProfileDiscriminatorError', 'ProfileFieldError', 'ProfileLibraryError', 'ProfileLinkedError', 'ProfileMaskError', 'ProfileOwnerError', 'ProfileTemporalError', 'PropertyCollisionError', 'PropertyOwnerError', 'PropertySelectionError', 'PropertyTypeError', 'QueryCycleError', 'QueryListError', 'RawProfileError', 'RecordExtentError', 'RequiredPointerNullError', 'RetainedRemovedError', 'RlayersSourceIdError', 'SameInputError', 'SeekTableError', 'SerializationError', 'Sha256WidthError', 'SnapshotMismatchError', 'SocketConstructorError', 'SocketOwnerError', 'OxblendSourceChangedError', 'SpillSpaceError', 'StorageProfileError', 'StrayDataRunError', 'StringNulError', 'StringTerminationError', 'TextConstructorError', 'TruncatedError', 'OxblendTypeError', 'TypedKindError', 'TypedOverflowError', 'TypedWidthError', 'UnknownConceptError', 'UnprofiledRoleError', 'UnresolvedPersistentError', 'UnsupportedEditError', 'UnsupportedFormatError', 'UnsupportedTypeError', 'UnverifiedSemanticsError', 'UsageError', 'ZipEncodingError', 'ZipPathError', 'ZipWidthError']
