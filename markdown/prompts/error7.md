13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] #[component]  
13:04:57 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:57 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:57 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:57 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:57 [linux]     rsx! {  
13:04:57 [linux]         button {  
13:04:57 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:57 [linux]             "{signal_owned_by_child}"  
13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] ```  
13:04:57 [linux] Fixed example ✅:  
13:04:57 [linux] ```rust  
13:04:57 [linux] #[component]  
13:04:57 [linux] fn Counters() -> Element {  
13:04:57 [linux]     let mut counts = use_signal(Vec::new);  
13:04:57 [linux]     rsx! {  
13:04:57 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:57 [linux]         button {  
13:04:57 [linux]             onclick: move |_| {  
13:04:57 [linux]                 counts.write().pop();  
13:04:57 [linux]             },  
13:04:57 [linux]             "Remove child"  
13:04:57 [linux]         }  
13:04:57 [linux]         "{counts:?}"  
13:04:57 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:57 [linux]         for index in 0..counts.len() {  
13:04:57 [linux]             Counter {  
13:04:57 [linux]                 index,  
13:04:57 [linux]                 counts  
13:04:57 [linux]             }  
13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] #[component]  
13:04:57 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:57 [linux]     rsx! {  
13:04:57 [linux]         button {  
13:04:57 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:57 [linux]             "{counts.read()[index]}"  
13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] ```  
13:04:57 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:57 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:57 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:57 [linux] Help:  
13:04:57 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:57 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:57 [linux] Broken example ❌:  
13:04:57 [linux] ```rust  
13:04:57 [linux] #[component]  
13:04:57 [linux] fn Counters() -> Element {  
13:04:57 [linux]     let counts = use_signal(Vec::new);  
13:04:57 [linux]     let mut children = use_signal(|| 0);  
13:04:57 [linux]     rsx! {  
13:04:57 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:57 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:57 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:57 [linux]         "{counts:?}"  
13:04:57 [linux]         for _ in 0..children() {  
13:04:57 [linux]             Counter {  
13:04:57 [linux]                 counts  
13:04:57 [linux]             }  
13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] #[component]  
13:04:57 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:57 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:57 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:57 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:57 [linux]     rsx! {  
13:04:57 [linux]         button {  
13:04:57 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:57 [linux]             "{signal_owned_by_child}"  
13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] ```  
13:04:57 [linux] Fixed example ✅:  
13:04:57 [linux] ```rust  
13:04:57 [linux] #[component]  
13:04:57 [linux] fn Counters() -> Element {  
13:04:57 [linux]     let mut counts = use_signal(Vec::new);  
13:04:57 [linux]     rsx! {  
13:04:57 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:57 [linux]         button {  
13:04:57 [linux]             onclick: move |_| {  
13:04:57 [linux]                 counts.write().pop();  
13:04:57 [linux]             },  
13:04:57 [linux]             "Remove child"  
13:04:57 [linux]         }  
13:04:57 [linux]         "{counts:?}"  
13:04:57 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:57 [linux]         for index in 0..counts.len() {  
13:04:57 [linux]             Counter {  
13:04:57 [linux]                 index,  
13:04:57 [linux]                 counts  
13:04:57 [linux]             }  
13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] #[component]  
13:04:57 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:57 [linux]     rsx! {  
13:04:57 [linux]         button {  
13:04:57 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:57 [linux]             "{counts.read()[index]}"  
13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] ```  
13:04:57 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:57 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:57 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:57 [linux] Help:  
13:04:57 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:57 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:57 [linux] Broken example ❌:  
13:04:57 [linux] ```rust  
13:04:57 [linux] #[component]  
13:04:57 [linux] fn Counters() -> Element {  
13:04:57 [linux]     let counts = use_signal(Vec::new);  
13:04:57 [linux]     let mut children = use_signal(|| 0);  
13:04:57 [linux]     rsx! {  
13:04:57 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:57 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:57 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:57 [linux]         "{counts:?}"  
13:04:57 [linux]         for _ in 0..children() {  
13:04:57 [linux]             Counter {  
13:04:57 [linux]                 counts  
13:04:57 [linux]             }  
13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] #[component]  
13:04:57 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:57 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:57 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:57 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:57 [linux]     rsx! {  
13:04:57 [linux]         button {  
13:04:57 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:57 [linux]             "{signal_owned_by_child}"  
13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] ```  
13:04:57 [linux] Fixed example ✅:  
13:04:57 [linux] ```rust  
13:04:57 [linux] #[component]  
13:04:57 [linux] fn Counters() -> Element {  
13:04:57 [linux]     let mut counts = use_signal(Vec::new);  
13:04:57 [linux]     rsx! {  
13:04:57 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:57 [linux]         button {  
13:04:57 [linux]             onclick: move |_| {  
13:04:57 [linux]                 counts.write().pop();  
13:04:57 [linux]             },  
13:04:57 [linux]             "Remove child"  
13:04:57 [linux]         }  
13:04:57 [linux]         "{counts:?}"  
13:04:57 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:57 [linux]         for index in 0..counts.len() {  
13:04:57 [linux]             Counter {  
13:04:57 [linux]                 index,  
13:04:57 [linux]                 counts  
13:04:57 [linux]             }  
13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] #[component]  
13:04:57 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:57 [linux]     rsx! {  
13:04:57 [linux]         button {  
13:04:57 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:57 [linux]             "{counts.read()[index]}"  
13:04:57 [linux]         }  
13:04:57 [linux]     }  
13:04:57 [linux] }  
13:04:57 [linux] ```  
13:04:57 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:173:16). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:775:31) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN Changing the props of `Style {}` is not supported    
13:04:58 [linux]  WARN A Copy Value created in ScopeId(105, "dioxus_flow::flow::Canvas") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:169:23). It will be dropped when that scope is  
dropped, but it was used in ScopeId(103, "dioxus_flow::flow::Flow<moonkale_editor_flow::panel::BlockData>") (at /home/daniel/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/dioxus-flow-0.1.2/src/flow.rs:728:34) which is not a desce  
ndant of the owning scope.  
13:04:58 [linux] This may cause reads or writes to fail because the value is dropped while it still held.  
13:04:58 [linux] Help:  
13:04:58 [linux] Copy values (like CopyValue, Signal, Memo, and Resource) are owned by the scope they are created in. If you use the value in a scope that may be dropped after the origin scope,  
13:04:58 [linux] it is very easy to use the value after it has been dropped. To fix this, you can move the value to the parent of all of the scopes that it is used in.  
13:04:58 [linux] Broken example ❌:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let counts = use_signal(Vec::new);  
13:04:58 [linux]     let mut children = use_signal(|| 0);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| children += 1, "Add child" }  
13:04:58 [linux]         button { onclick: move |_| children -= 1, "Remove child" }  
13:04:58 [linux]         // A signal from a child is read or written to in a parent scope  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         for _ in 0..children() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(mut counts: Signal<Vec<Signal<i32>>>) -> Element {  
13:04:58 [linux]     let mut signal_owned_by_child = use_signal(|| 0);  
13:04:58 [linux]     // Moving the signal up to the parent may cause issues if you read the signal after the child scope is dropped  
13:04:58 [linux]     use_hook(|| counts.push(signal_owned_by_child));  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| signal_owned_by_child += 1,  
13:04:58 [linux]             "{signal_owned_by_child}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:58 [linux] Fixed example ✅:  
13:04:58 [linux] ```rust  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counters() -> Element {  
13:04:58 [linux]     let mut counts = use_signal(Vec::new);  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button { onclick: move |_| counts.write().push(0), "Add child" }  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| {  
13:04:58 [linux]                 counts.write().pop();  
13:04:58 [linux]             },  
13:04:58 [linux]             "Remove child"  
13:04:58 [linux]         }  
13:04:58 [linux]         "{counts:?}"  
13:04:58 [linux]         // Instead of passing up a signal, we can just write to the signal that lives in the parent  
13:04:58 [linux]         for index in 0..counts.len() {  
13:04:58 [linux]             Counter {  
13:04:58 [linux]                 index,  
13:04:58 [linux]                 counts  
13:04:58 [linux]             }  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] #[component]  
13:04:58 [linux] fn Counter(index: usize, mut counts: Signal<Vec<i32>>) -> Element {  
13:04:58 [linux]     rsx! {  
13:04:58 [linux]         button {  
13:04:58 [linux]             onclick: move |_| counts.write()[index] += 1,  
13:04:58 [linux]             "{counts.read()[index]}"  
13:04:58 [linux]         }  
13:04:58 [linux]     }  
13:04:58 [linux] }  
13:04:58 [linux] ```  
13:04:59 [dev] Application [linux] exited with error: signal: 9 (SIGKILL)