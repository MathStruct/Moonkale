// One document-level FontFaceSet listener shared by mounted geometry views.
// Observers/listeners are released when their last mounted node disappears.
const {id} = await dioxus.recv();
const node = document.getElementById(id);
const fonts = document.fonts;
if (!node || !fonts) {
    dioxus.send({epoch:0, loading:false, closed:true});
} else {
    const key = Symbol.for("moonkale.geometry.font-watch");
    let pool = window[key];
    if (!pool) {
        pool = {epoch:0, watchers:new Map(), disposed:false};
        const faces = new WeakMap();
        let nextFace = 0;
        pool.fontState = () => {
            const state = [fonts.status];
            fonts.forEach(face => {
                if (!faces.has(face)) faces.set(face, ++nextFace);
                state.push([faces.get(face),face.family,face.status,face.weight,face.style,face.stretch,face.unicodeRange].join("|"));
            });
            return state.join(";");
        };
        pool.fingerprint = pool.fontState();
        pool.cleanup = () => {
            if (pool.watchers.size || pool.disposed) return;
            pool.disposed = true;
            pool.observer.disconnect();
            clearInterval(pool.poll);
            for (const type of ["loading", "loadingdone", "loadingerror"]) fonts.removeEventListener(type, pool.changed);
            if (window[key] === pool) delete window[key];
        };
        pool.changed = () => {
            if (pool.disposed) return;
            pool.fingerprint = pool.fontState();
            pool.epoch += 1;
            for (const watcher of pool.watchers.values()) watcher.send({epoch:pool.epoch, loading:fonts.status === "loading", closed:false});
        };
        pool.observer = new MutationObserver(() => {
            for (const [id, watcher] of pool.watchers) {
                if (!watcher.node.isConnected || document.getElementById(id) !== watcher.node) {
                    pool.watchers.delete(id);
                    watcher.close();
                }
            }
            pool.cleanup();
            if (!pool.disposed && pool.fontState() !== pool.fingerprint) pool.changed();
        });
        pool.observer.observe(document.documentElement, {childList:true, subtree:true});
        for (const type of ["loading", "loadingdone", "loadingerror"]) fonts.addEventListener(type, pool.changed);
        // Already loaded/fast local faces need not produce loading events.
        // One cheap metadata check per document also catches add/delete/replace.
        pool.poll = setInterval(() => {
            if (!pool.disposed && pool.fontState() !== pool.fingerprint) pool.changed();
        }, 500);
        window[key] = pool;
        fonts.ready.then(pool.changed);
    }
    await new Promise(resolve => {
        const previous = pool.watchers.get(id);
        if (previous) previous.close();
        const watcher = {
            node,
            send: value => { try { dioxus.send(value); } catch { pool.watchers.delete(id); resolve(); pool.cleanup(); } },
            close: () => { try { dioxus.send({epoch:pool.epoch,loading:false,closed:true}); } catch {} resolve(); }
        };
        pool.watchers.set(id, watcher);
        watcher.send({epoch:pool.epoch,loading:fonts.status === "loading",closed:false});
    });
}
