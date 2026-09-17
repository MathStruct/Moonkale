//! `Document`: rope + version + undo stack + node id. Converts backend edit events into `core` patches and applies incoming `SourceEvent`s (external change → reconcile or prompt).
