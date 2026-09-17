This is a big Dioxus project. [[AGENTS]] tells you about how to use this project.

The goal is to create a graph-native code and knowledge editor, that can open projects from both files in a folder and rom database coonnections. You can take a look at [[rough goal]] to see the description for the old Lumino/TS/Tauri project. 

Please keep as much as possible to the Rust ecosystem. You may import and use Typescript packages such as Code Mirror or Milkdown. But please keep them modular such that if a Rust alternative arises they are easily replaced by the upcoming Rust library.

Please make it that all targets i.e. desktop, web and mobile are served. The resulting app does not have to be identical but it should consider on how this works on the target platform. 
If a feature (both frontend and backend) only runs only on one platform please separate it accordingly from the rest of the project.

Please assume that one can connect multiple database sources:
- SQL-like: Postgres/Supabase, TursoDB, DuckDB, SQLite
- Graph-database:  TypeDB, LadyBugDB, HelixDB, FalkorDB
- Key-Value: Redis, Dragonfly.
- maybe others.
and view and display their content. 
Assume that people might open a folder on their PC.
Assume that people might open a terminal
Assume that there are Language servers running.
Assume that LLM's are querying the opened projects and are both running queries (SQL,Cypher, ...) as well as requesting embeddings.
Example use cases:
- An editor for Unison Language
- A acuasal modeling editor / editor for machine learning for ModelingToolkit.jl and Lux.jl (Basically I want to build machine learning models for Lux.jl with Drag and drop)
- an extensive wiki for Bioinformatics/Metabolomics/Proteomics/Genetics which contains lots of markdown/typst pages, diagrams, as well as computational information (i.e. source code) 
- mixed knowledge and code graphs
- visualizing stacktraces of program errors. ASTs of functions, variables, types.


Please assume multiple differrent windows:
- Markdown/Typst editing:
	- a WYSIWYG window if possible that allows to
	- Obsidian style connections
- Code editing:
	- Able to display code of programming languages.
	- Start with maybe Rust, Julia, Go, Unison, Lean
- SQL like editor/ table editor
- Graph view/Graph Database view
	- Please go into different options: 
		- A GPU native render (Obsidians problem is over a certain size the graph renders very slowly. So I extensively care about fluid rendering of the graph.)
		- a popup view that displays the content of edges/nodes
		- arrows that show different colors or directions. Display of subgraphs
		- 3D display of graphs
- No-Code/Low-code interface where one can drag and drop elements and wire them.
- Terminal window.

Please extensively document how to write extensions for the project. Assume that this project is mainly extension driven and document how to write one.


Please create an extensive markdown vault. Document every design choice if a problem during the implementation arises put it in the markdown vault. The markdown vault is an obsidian vault and currently has various plugins installed.
Rank problems according to difficulty and give a recommendation in which order you want to implement them. You may include diagrams that display design choices using mermaid or TikZ or Excalidraw.

Currently the Dioxus project just contains a dummy UI. Please suggest a project structure and start a bunch of Rust files. Please do not implement the Rust code but just fill these Rust files with comments on what they are supposed to do. Please create a parallel markdown file that explains your rationale.