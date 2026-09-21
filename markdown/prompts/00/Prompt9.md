Currently it is that one could open multiple folders at once. 

I initially thought that is a bug, however now I think that this is actually good design. 
It should be possible to habe multiple sources open at once:
Sources can be:
- Folders (multiple)
- Folders behind ssh connections (Like the Zed editor)
- Github/GitLab/Codeberg repositories
- Database connections
- API queryable information (if for example the database is behind a REST-API)

Additionally:
- some sources are writeable some are just readable (like other peoples github repositories).
- A project can have multiple sources open. One should be able to open and close sources individually.
- if documents from a source are open one should plan for different color schemes maybe to distinguish them.

However I would like to have a selector of projects which one is working on.
The project is not a folder (or folder bound) it is a saved internal persitent state of Moonkale. 
It needs a mechanism to synchronize projects and import/export them. (And also return a failure notice if for example a folder does no exist.)

Please document this desired behaviour first.