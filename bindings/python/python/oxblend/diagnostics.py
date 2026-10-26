"""Owned, local source evidence. Share only the report API's sanitized projection.

``SourceDiagnostic`` snapshots the private local paths, messages and check
provenance behind one raised ``OxblendError``. It is explicitly library-
unbound: no ledger ID and no CLI invocation receipt is fabricated. The
``report`` operation (``oxblend report``) applies the same closed privacy
projection as the CLI before producing a bundle, so only its sanitized
preview may leave the machine.
"""
from __future__ import annotations
from . import Record


class SourceContext(Record):
    """Saved source-site observations, not reconstructed or evaluated state.

    Paths/names are native-byte hex, addresses are decimal strings. Optional
    fields remain None when the producing check did not observe them. The
    constructor takes the facts mapping to own, like ``Record``.

    Args:
        facts: the observed source-site facts mapping to own, like ``Record``.

    Returns:
        SourceContext: the owned immutable mapping over the observations.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: in-process view of the source-site facts the
    ``oxblend report`` bundle sanitizes; raw contexts never leave the machine.
    """
    __slots__ = ()
    @property
    def file_hex(self) -> str | None:
        """The ``file_hex`` observation: native source-path bytes as hex.

        Returns:
            Lowercase hex of the observed path, or None when this check
            observed no file.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['file_hex']
    @property
    def block_index(self) -> int | None:
        """The ``block_index`` observation: physical block index observed.

        Returns:
            The physical block index, or None when this check observed no
            single block.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['block_index']
    @property
    def block_code_hex(self) -> str | None:
        """The ``block_code_hex`` observation: four code bytes as hex.

        Returns:
            Lowercase hex of the observed code including NUL padding, or None
            when this check observed no block code.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['block_code_hex']
    @property
    def old_address(self) -> str | None:
        """The ``old_address`` observation: saved pointer as decimal text.

        Returns:
            The decimal address string, or None when this check observed no
            address.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['old_address']
    @property
    def sdna_index(self) -> int | None:
        """The ``sdna_index`` observation: DNA type index observed.

        Returns:
            The SDNA type index, or None when this check observed no typed
            record.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['sdna_index']
    @property
    def count(self) -> int | None:
        """The ``count`` observation: struct element count observed.

        Returns:
            The element count, or None when this check observed no counted
            allocation.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['count']
    @property
    def payload_size(self) -> int | None:
        """The ``payload_size`` observation: payload byte length observed.

        Returns:
            The payload size in bytes, or None when this check observed no
            payload extent.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['payload_size']
    @property
    def data_offset(self) -> int | None:
        """The ``data_offset`` observation: payload byte offset observed.

        Returns:
            The byte offset of the payload, or None when this check observed
            no located record.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['data_offset']
    @property
    def element(self) -> int | None:
        """The ``element`` observation: struct element index observed.

        Returns:
            The element index, or None when this check observed no single
            element.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['element']
    @property
    def offset_in_block(self) -> int | None:
        """The ``offset_in_block`` observation: byte offset in the payload.

        Returns:
            The byte offset within the block payload, or None when this check
            observed no intra-block site.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['offset_in_block']
    @property
    def decoded_file_offset(self) -> int | None:
        """The ``decoded_file_offset`` observation: decoded file position.

        Returns:
            The decoded file offset, or None when this check observed no
            positioned record.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['decoded_file_offset']
    @property
    def width(self) -> int | None:
        """The ``width`` observation: field or character width observed.

        Returns:
            The width in bytes or characters, or None when this check observed
            no width-bound site.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['width']
    @property
    def blend_version(self) -> int | None:
        """The ``blend_version`` observation: saved format version seen.

        Returns:
            The saved format version, or None when this check observed no
            header.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['blend_version']
    @property
    def little_endian(self) -> bool | None:
        """The ``little_endian`` observation: saved endianness seen.

        Returns:
            True for little-endian saves, False for big-endian, or None when
            this check observed no header.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['little_endian']
    @property
    def field_path_hex(self) -> tuple[str, ...]:
        """The ``field_path_hex`` observation: field-path steps as hex.

        Returns:
            The hex-encoded field-name steps in path order.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['field_path_hex']
    @property
    def pointer_decimal(self) -> str | None:
        """The ``pointer_decimal`` observation: pointer value as decimal.

        Returns:
            The decimal pointer string, or None when this check observed no
            pointer.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['pointer_decimal']
    @property
    def namespace(self) -> str:
        """The ``namespace`` of this observation: which pointer space applies.

        Returns:
            The namespace name the observation belongs to.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['namespace']
    @property
    def resolution(self) -> str:
        """The ``resolution`` of this observation: how the site resolved.

        Returns:
            The resolution outcome recorded by the producing check.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the source-site facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['resolution']


class SourcePropagation(Record):
    """A later owning check's typed observation carried by the same Error.

    Propagations record how the original source snapshot moved through later
    owning checks; each carries its own kind, check identity and context.
    The constructor takes the facts mapping to own, like ``Record``.

    Args:
        facts: the propagated observation facts mapping to own, like ``Record``.

    Returns:
        SourcePropagation: the owned immutable mapping over the observation.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: in-process view of the propagation facts the
    ``oxblend report`` bundle sanitizes.
    """
    __slots__ = ()
    @property
    def kind(self) -> str:
        """The propagated observation's stable kind string.

        Returns:
            The kind recorded by the later owning check.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the propagation facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['kind']
    @property
    def check_id(self) -> str:
        """The later owning check that carried this observation.

        Returns:
            The check identity string.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the propagation facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['check_id']
    @property
    def context(self) -> SourceContext:
        """The source-site context observed by the later check.

        Returns:
            The typed ``SourceContext`` for this propagation.

        Raises:
            KeyError: the snapshot omits the context (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the propagation facts the
        ``oxblend report`` bundle sanitizes.
        """
        return SourceContext(self['context'])


class SourceDiagnostic(Record):
    """An Error-owned source snapshot; never a CLI invocation receipt.

    Contains private local paths and messages. report(error, output) applies
    the same closed privacy projection as the CLI before producing a bundle.
    The constructor takes the facts mapping to own, like ``Record``.

    Args:
        facts: the snapshot facts mapping to own, like ``Record``.

    Returns:
        SourceDiagnostic: the owned immutable mapping over the snapshot facts.

    Raises:
        AttributeError: ``facts`` is not a mapping.
        TypeError: a fact value is not an immutable scalar, mapping or sequence.

    No CLI equivalent: in-process view of the snapshot facts the
    ``oxblend report`` bundle sanitizes; the CLI never prints raw snapshots.
    """
    __slots__ = ()
    @property
    def kind(self) -> str:
        """The snapshot's stable error-kind string.

        Returns:
            The kind recorded on the owning error.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the snapshot facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['kind']
    @property
    def message(self) -> str:
        """The raw local message kept on the owning error.

        Returns:
            The unsanitized message text; never shared, only the report
            preview's sanitized projection may leave the machine.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: raw messages stay local; ``oxblend report``
        publishes only its sanitized projection.
        """
        return self['message']
    @property
    def check_id(self) -> str:
        """The check that captured this snapshot.

        Returns:
            The check identity string.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the snapshot facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['check_id']
    @property
    def context_origin(self) -> str:
        """Where the snapshot's context was captured from.

        Returns:
            The context-origin marker recorded by the producing check.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the snapshot facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['context_origin']
    @property
    def context(self) -> SourceContext:
        """The source-site context captured with this snapshot.

        Returns:
            The typed ``SourceContext`` for this snapshot.

        Raises:
            KeyError: the snapshot omits the context (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the snapshot facts the
        ``oxblend report`` bundle sanitizes.
        """
        return SourceContext(self['context'])
    @property
    def caused_by(self) -> tuple[str, ...]:
        """The causal check identities behind this snapshot.

        Returns:
            The causing check identities in order.

        Raises:
            KeyError: the snapshot omits this observation (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the snapshot facts the
        ``oxblend report`` bundle sanitizes.
        """
        return self['caused_by']
    @property
    def propagations(self) -> tuple[SourcePropagation, ...]:
        """The later owning checks' observations carried by this error.

        Returns:
            One ``SourcePropagation`` per carrying check, in order.

        Raises:
            KeyError: the snapshot omits propagations (never for
                engine-produced snapshots).

        No CLI equivalent: in-process view of the snapshot facts the
        ``oxblend report`` bundle sanitizes.
        """
        return tuple(SourcePropagation(row) for row in self['propagations'])


__all__ = ['SourceContext', 'SourcePropagation', 'SourceDiagnostic']
