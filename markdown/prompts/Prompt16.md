## Note about Julia

I know most people are not using Julia. however i do. So I somewhat want it part of Moonkale, however maybe avoid it in the core package. 

Although there is Julia-WASM it should not be part of the standard web version of Moonkale.

When it comes to Julia a Project.toml and a Manifest.toml exist, I would like to have the relevant files from ~/.julia/packages to be added and indexed as a read-only source.


I would like to have decent editors for:
- Lux.jl (a DAG) just the flow editor
- ModelingToolkit.jl would also be a decent as a drag and drop Simulink like editor.

However I really want to have a graphical editor for Lenticulum.jl

The whole reaason I care about Moonkale and started that project is that I actually want to build https://github.com/MathStruct/Lenticulum.jl or https://mathstruct.org/Lenticulum.jl/dev/vault/ an implicit machine learning library.

However this will yield a huge graph of interconnections and I don't want to deal with pages of declarations of graph connections. Which is why I really want an editor.

In my opinion I do not want something that is a diagram to be presented as a declarative text file to a human. I want it to remain a diagram.

It is fine for JSON because JSON is a treeof statements. However compare it to RDF and I don't want to expose humans to raw RDF files, but to the graph behind it is a lesser problem.

Sofar people have not suffered enough with Tensorflow or Pytorch to build a drag and drop neural network editor, however when it comes to watching big Factorgraphs as expected for Lenticulum.jl/Mycelium.jl then I definitely want this in an editor.

Further specification about this incoming later.
But just imagine:
- Rendering a graph of a metabolomics model
- Redering factor graph of current state of a SLAM problem
- Rendering state of market model
That would be terrible.

Please consider atleast documenting this as a goal, even if I cannot specify Lenticulum.jl yet.