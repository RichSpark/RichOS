#![cfg(unix)]
use richos_core::{entity::{Entity, EntityId, EntityRegistry, RegistrySource}, runtime::EngineRuntime};
use richos_core::repositories::connect_folder as connect;
use std::{path::PathBuf, collections::BTreeMap, process::Command};
struct Fixture(PathBuf);
impl Fixture {
 fn new() -> Self { let p=std::env::temp_dir().join(format!("richos repo fixture {}",uuid::Uuid::new_v4()));std::fs::create_dir(&p).unwrap();Self(std::fs::canonicalize(p).unwrap()) }
 fn runtime(&self)->EngineRuntime { EngineRuntime{root:self.0.clone(),git:"/usr/bin/git".into(),python:"/unused/python".into(),node:"/unused/node".into(),versions:BTreeMap::new()} }
 fn dir(&self,name:&str)->PathBuf {let p=self.0.join(name);std::fs::create_dir(&p).unwrap();p}
 fn registry(&self)->EntityRegistry {EntityRegistry::new(vec![Entity::new("alpha","Alpha",&[]).unwrap(),Entity::new("beta","Beta",&[]).unwrap()]).unwrap()}
}
impl Drop for Fixture {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
fn id(s:&str)->EntityId {EntityId::parse(s).unwrap()}
#[test]
fn two_repositories_persist_as_explicit_connections_and_leave_dirty_files_alone(){
 let f=Fixture::new();let one=f.dir("project one");let two=f.dir("project two");
 let (reg,first)=connect(&f.registry(),&id("alpha"),&one,&f.runtime(),&[]).unwrap();assert!(first.initialized);
 std::fs::write(one.join("local.txt"),"unrelated local edit").unwrap();
 let (reg,_)=connect(&reg,&id("alpha"),&two,&f.runtime(),&[]).unwrap();
 let (reg,again)=connect(&reg,&id("alpha"),&one,&f.runtime(),&[]).unwrap();assert!(!again.initialized);
 assert_eq!(std::fs::read_to_string(one.join("local.txt")).unwrap(),"unrelated local edit");
 reg.save(&f.0.join("entities.json")).unwrap();let loaded=EntityRegistry::load(&f.0.join("entities.json"));
 assert_eq!(loaded.source,RegistrySource::File);assert_eq!(loaded.registry,reg);
 assert_eq!(reg.get(&id("alpha")).unwrap().connected_repositories.len(),2);
 assert!(reg.get(&id("beta")).unwrap().connected_repositories.is_empty());
}
#[test]
fn refusal_never_initializes_missing_or_wrong_company_directories(){
 // Every refusal still comes before Git is touched (CEO, 2026-10-06): another company's
 // folder, empty or full, and a folder that does not exist.
 let f=Fixture::new();let empty=f.dir("empty");let full=f.dir("full");std::fs::write(full.join("keep"),"value").unwrap();
 let reg=EntityRegistry::new(vec![Entity::try_new("alpha","Alpha",vec![]).unwrap(),Entity::try_new("beta","Beta",vec![empty.clone(),full.clone()]).unwrap()]).unwrap();
 assert!(connect(&reg,&id("alpha"),&empty,&f.runtime(),&[]).unwrap_err().contains("overlaps"));
 assert!(connect(&reg,&id("alpha"),&full,&f.runtime(),&[]).unwrap_err().contains("overlaps"));
 assert!(connect(&reg,&id("alpha"),&f.0.join("missing"),&f.runtime(),&[]).unwrap_err().contains("existing folder"));
 assert!(!empty.join(".git").exists());assert!(!full.join(".git").exists());assert!(!f.0.join("missing").exists());
}
fn git_out(dir:&std::path::Path,args:&[&str])->String{
 let o=Command::new("/usr/bin/git").current_dir(dir).env("GIT_CONFIG_GLOBAL","/dev/null").env("GIT_CONFIG_NOSYSTEM","1").args(args).output().unwrap();
 assert!(o.status.success(),"git {args:?}: {}",String::from_utf8_lossy(&o.stderr));String::from_utf8(o.stdout).unwrap().trim().to_string()
}
#[test]
fn a_folder_without_git_gets_git_and_its_files_become_the_first_commit(){
 // CEO, 2026-10-06: every connected folder always gets Git tracking. `git add -A` respects
 // the folder's own `.gitignore`; nothing is pushed and no remote is added.
 let f=Fixture::new();let dir=f.dir("my notes");
 std::fs::write(dir.join("plan.md"),"the plan\n").unwrap();std::fs::create_dir(dir.join("drafts")).unwrap();
 std::fs::write(dir.join("drafts").join("one.txt"),"draft\n").unwrap();
 std::fs::write(dir.join(".gitignore"),"secret.env\n").unwrap();std::fs::write(dir.join("secret.env"),"TOKEN=placeholder\n").unwrap();
 let (reg,repo)=connect(&f.registry(),&id("alpha"),&dir,&f.runtime(),&[]).unwrap();
 assert!(repo.initialized,"the folder had no Git, so this connect set it up");assert_eq!(repo.branch,"main");
 assert_eq!(git_out(&dir,&["ls-files"]),".gitignore\ndrafts/one.txt\nplan.md");
 assert_eq!(git_out(&dir,&["rev-list","--count","HEAD"]),"1");
 assert_eq!(git_out(&dir,&["status","--porcelain"]),"","every file not ignored is in the first commit");
 assert_eq!(git_out(&dir,&["remote"]),"","no remote is added");
 assert!(git_out(&dir,&["config","--list","--local"]).lines().all(|l|!l.starts_with("richos.")),"the set-up marker is cleared");
 assert_eq!(std::fs::read_to_string(dir.join("plan.md")).unwrap(),"the plan\n");
 assert_eq!(std::fs::read_to_string(dir.join("secret.env")).unwrap(),"TOKEN=placeholder\n");
 assert_eq!(reg.get(&id("alpha")).unwrap().connected_repositories,vec![dir.clone()]);
}
/// Every file under `dir`, outside `.git`, by its path relative to `dir`.
fn files_under(dir:&std::path::Path)->Vec<PathBuf>{
 let mut out=vec![];let mut todo=vec![dir.to_path_buf()];
 while let Some(d)=todo.pop(){for e in std::fs::read_dir(&d).unwrap().flatten(){
  let p=e.path();if p.file_name().is_some_and(|n|n==".git"){continue;}
  if p.is_dir(){todo.push(p);}else{out.push(p.strip_prefix(dir).unwrap().to_path_buf());}
 }}
 out.sort();out
}
#[test]
fn connecting_leaves_the_folders_own_instruction_files_exactly_as_they_were(){
 // A connected folder's own CLAUDE.md, AGENTS.md and CLAUDE.local.md are the user's, for the
 // terminal as much as for the app (richos-hq docs/plans/2026-10-21-folder-instruction-files.md
 // §3 item 1): connecting never writes, moves, renames or deletes one, and adds none anywhere.
 let f=Fixture::new();let dir=f.dir("their project");
 let files=[("CLAUDE.md","# House rules\nEvery commit message ends with [house-rule].\n"),
            ("AGENTS.md","Agents: keep changes small.\n"),("CLAUDE.local.md","My own notes, never shared.\n")];
 for (name,text) in files {std::fs::write(dir.join(name),text).unwrap();}
 std::fs::write(dir.join("plan.md"),"the plan\n").unwrap();
 let before=files_under(&dir);
 let (_,repo)=connect(&f.registry(),&id("alpha"),&dir,&f.runtime(),&[]).unwrap();
 assert!(repo.initialized,"the folder had no Git, so this connect set it up");
 for (name,text) in files {assert_eq!(std::fs::read_to_string(dir.join(name)).unwrap(),text,"{name} changed");}
 assert_eq!(files_under(&dir),before,"connecting added, moved or removed a file outside .git");
 assert!(!dir.join(".claude").exists(),"connecting made a .claude folder");
}
#[test]
fn an_empty_folder_gets_git_with_an_empty_first_commit(){
 let f=Fixture::new();let dir=f.dir("fresh");
 let (_,repo)=connect(&f.registry(),&id("alpha"),&dir,&f.runtime(),&[]).unwrap();assert!(repo.initialized);
 assert_eq!(git_out(&dir,&["ls-files"]),"");assert_eq!(git_out(&dir,&["rev-list","--count","HEAD"]),"1");
}
#[test]
fn an_existing_repository_connects_as_it_is(){
 let f=Fixture::new();let dir=f.dir("theirs");
 git_out(&dir,&["init","-q","--initial-branch=trunk"]);std::fs::write(dir.join("a.txt"),"one\n").unwrap();
 git_out(&dir,&["add","a.txt"]);git_out(&dir,&["-c","user.name=T","-c","user.email=t@example.invalid","commit","-q","-m","theirs"]);
 let head=git_out(&dir,&["rev-parse","HEAD"]);std::fs::write(dir.join("a.txt"),"local edit\n").unwrap();std::fs::write(dir.join("new.txt"),"untracked\n").unwrap();
 let (_,repo)=connect(&f.registry(),&id("alpha"),&dir,&f.runtime(),&[]).unwrap();
 assert!(!repo.initialized);assert_eq!(repo.branch,"trunk");
 assert_eq!(git_out(&dir,&["rev-parse","HEAD"]),head,"no commit is added to an existing repository");
 assert_eq!(git_out(&dir,&["status","--porcelain"]),"M a.txt\n?? new.txt","local changes stay where they were");
}
#[test]
fn a_set_up_cut_short_is_finished_and_a_repository_it_did_not_start_is_not_committed(){
 let f=Fixture::new();
 // RichOS's own set-up, killed after `init`: the marker says so, and the next connect finishes it.
 let ours=f.dir("ours");std::fs::write(ours.join("kept.txt"),"kept\n").unwrap();
 git_out(&ours,&["init","-q","--initial-branch=main"]);git_out(&ours,&["config","richos.initializing","true"]);
 let (_,repo)=connect(&f.registry(),&id("alpha"),&ours,&f.runtime(),&[]).unwrap();
 assert!(repo.initialized);assert_eq!(git_out(&ours,&["ls-files"]),"kept.txt");
 assert!(git_out(&ours,&["config","--list","--local"]).lines().all(|l|!l.starts_with("richos.")));
 // His own `git init` with files and no commit yet: his to commit, refused as before.
 let theirs=f.dir("theirs");std::fs::write(theirs.join("work.txt"),"work\n").unwrap();git_out(&theirs,&["init","-q"]);
 assert!(connect(&f.registry(),&id("alpha"),&theirs,&f.runtime(),&[]).unwrap_err().contains("no initial commit"));
 assert!(Command::new("/usr/bin/git").current_dir(&theirs).args(["rev-parse","--verify","-q","HEAD"]).status().unwrap().code()!=Some(0));
}
#[test]
fn a_failed_set_up_leaves_no_git_behind(){
 let f=Fixture::new();let dir=f.dir("locked");let locked=dir.join("unreadable.txt");std::fs::write(&locked,"x").unwrap();
 use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&locked,std::fs::Permissions::from_mode(0o000)).unwrap();
 let refused=connect(&f.registry(),&id("alpha"),&dir,&f.runtime(),&[]);
 std::fs::set_permissions(&locked,std::fs::Permissions::from_mode(0o600)).unwrap();
 assert!(refused.is_err(),"Git cannot read the file, so the first commit cannot be made");
 assert!(!dir.join(".git").exists(),"the .git this connect made is removed again");
}
#[test]
fn nested_repositories_and_worktrees_are_not_adopted_as_main_checkouts(){
 let f=Fixture::new();let root=f.dir("main");let (reg,_)=connect(&f.registry(),&id("alpha"),&root,&f.runtime(),&[]).unwrap();
 let nested=root.join("nested");std::fs::create_dir(&nested).unwrap();
 assert!(connect(&reg,&id("alpha"),&nested,&f.runtime(),&[]).is_err());
 let linked=f.0.join("linked");assert!(Command::new("/usr/bin/git").current_dir(&root).args(["worktree","add","-q","-b","worker"]).arg(&linked).status().unwrap().success());
 assert!(connect(&reg,&id("alpha"),&linked,&f.runtime(),&[]).is_err());
}
#[test]
fn engine_storage_and_other_company_aliases_are_refused(){
 let f=Fixture::new();let root=f.dir("private app");
 assert!(connect(&f.registry(),&id("alpha"),&root,&f.runtime(),&[&root]).is_err());
 let alias=f.0.join("alias");std::os::unix::fs::symlink(&root,&alias).unwrap();
 let reg=EntityRegistry::new(vec![Entity::new("alpha","Alpha",&[]).unwrap(),Entity::try_new("beta","Beta",vec![alias]).unwrap()]).unwrap();
 assert!(connect(&reg,&id("alpha"),&root,&f.runtime(),&[]).is_err());assert!(!root.join(".git").exists());
}
#[test]
fn legacy_registry_loads_without_granting_repository_execution(){
 let f=Fixture::new();let p=f.0.join("legacy.json");std::fs::write(&p,r#"{"version":1,"entities":[{"id":"alpha","display_name":"Alpha","roots":["/fictional/repo"]}]}"#).unwrap();
 let original=std::fs::read(&p).unwrap();
 let loaded=EntityRegistry::load(&p);assert_eq!(loaded.source,RegistrySource::File);assert!(loaded.registry.get(&id("alpha")).unwrap().connected_repositories.is_empty());
 loaded.registry.save(&p).unwrap();
 let backups:Vec<_>=std::fs::read_dir(&f.0).unwrap().flatten().filter(|e|e.file_name().to_string_lossy().ends_with(".backup.json")).collect();
 assert_eq!(backups.len(),1);assert_eq!(std::fs::read(backups[0].path()).unwrap(),original);
}
#[test]
fn a_repository_inside_the_owners_company_folder_can_still_be_shared_with_another_company(){
 // Plan item 11, §138. Alpha's company folder is the parent and Alpha has connected a
 // repository inside it, so Alpha owns BOTH roots. Beta may still connect that repository;
 // any other overlap with Alpha's folder is still refused.
 let f=Fixture::new();let home=f.dir("alpha home");let project=home.join("project");std::fs::create_dir(&project).unwrap();
 let other=home.join("other");std::fs::create_dir(&other).unwrap();
 let reg=EntityRegistry::new(vec![Entity::try_new("alpha","Alpha",vec![home.clone()]).unwrap(),Entity::new("beta","Beta",&[]).unwrap()]).unwrap();
 let (reg,_)=connect(&reg,&id("alpha"),&project,&f.runtime(),&[]).unwrap();
 let (reg,_)=connect(&reg,&id("beta"),&project,&f.runtime(),&[]).unwrap();
 assert_eq!(reg.get(&id("beta")).unwrap().connected_repositories,vec![project.clone()]);
 assert!(reg.get(&id("beta")).unwrap().roots.is_empty(),"roots stay exclusive to Alpha");
 assert_eq!(reg.resolve_root(&project).unwrap().id,id("alpha"));
 assert!(connect(&reg,&id("beta"),&other,&f.runtime(),&[]).unwrap_err().contains("overlaps"),"an unconnected folder in Alpha's folder is still refused");
 assert!(connect(&reg,&id("beta"),&home,&f.runtime(),&[]).unwrap_err().contains("overlaps"),"Alpha's company folder itself is still refused");
 // The registry survives a save and a load.
 reg.save(&f.0.join("entities.json")).unwrap();
 assert_eq!(EntityRegistry::load(&f.0.join("entities.json")).registry,reg);
}
