// Geometry only: share one ResizeObserver, skip its redundant initial entries,
// and notify Rust only when a mounted run's dimensions actually change.
const {id} = await dioxus.recv();
const node = document.getElementById(id);
if (node) {
    const key = Symbol.for('moonkale.geometry.resize-watch');
    let pool = window[key];
    if (!pool || pool.disposed) {
        pool = {watchers:new Map(), nodes:new WeakMap(), disposed:false};
        pool.cleanup = () => {
            if (pool.watchers.size || pool.disposed) return;
            pool.disposed = true;
            pool.resize.disconnect();
            pool.removals.disconnect();
            if (window[key] === pool) delete window[key];
        };
        pool.resize = new ResizeObserver(entries => {
            for (const entry of entries) {
                const watcher = pool.nodes.get(entry.target);
                if (!watcher || !entry.target.isConnected) continue;
                const box = entry.borderBoxSize?.[0] ?? entry.borderBoxSize;
                const width = box?.inlineSize ?? entry.contentRect.width;
                const height = box?.blockSize ?? entry.contentRect.height;
                if (!Number.isFinite(width) || !Number.isFinite(height)) continue;
                const previous = watcher.size;
                watcher.size = {width,height};
                // Mounted source runs already request their initial geometry.
                if (previous && (Math.abs(previous.width-width)>1e-6 || Math.abs(previous.height-height)>1e-6)) {
                    try { watcher.send({width,height}); } catch { watcher.close(); }
                }
            }
        });
        pool.removals = new MutationObserver(() => {
            for (const watcher of pool.watchers.values()) {
                if (!watcher.node.isConnected || document.getElementById(watcher.id) !== watcher.node) watcher.close();
            }
        });
        pool.removals.observe(document.documentElement,{childList:true,subtree:true});
        window[key] = pool;
    }
    await new Promise(resolve => {
        const previous = pool.watchers.get(id);
        const watcher = {id,node,size:null,closed:false,send:value=>dioxus.send(value),close:()=>{
            if (watcher.closed) return;
            watcher.closed = true;
            if (pool.nodes.get(node) === watcher) {
                pool.resize.unobserve(node);
                pool.nodes.delete(node);
            }
            if (pool.watchers.get(id) === watcher) pool.watchers.delete(id);
            resolve();
            pool.cleanup();
        }};
        pool.watchers.set(id,watcher);
        previous?.close(); // The new entry keeps the pool alive during replacement.
        pool.nodes.set(node,watcher);
        pool.resize.observe(node);
    });
}
