# Standard Git Procedures

The codebase for all things software for Rice Eclipse is on GitHub ([https://github.com/rice-eclipse](https://github.com/rice-eclipse)). This document contains all relevant procedures and information on how to appropriately use Git version control for our software development.

We will also be using GitHub Issues to keep track of specific tasks that we come up with.

## Git Installation and Setup

1. [Install Git](https://git-scm.com/downloads)
2. [Configure username](https://docs.github.com/en/get-started/getting-started-with-git/setting-your-username-in-git)
3. [Configure commit email address](https://docs.github.com/en/account-and-profile/setting-up-and-managing-your-personal-account-on-github/managing-email-preferences/setting-your-commit-email-address#setting-your-commit-email-address-in-git)

## Setting up SSH key (if you haven’t already)

Use the Git Bash terminal application. Follow these instructions:

1. [Check for existing SSH keys](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/checking-for-existing-ssh-keys):
2. [Generate new SSH key and add to SSH agent](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/generating-a-new-ssh-key-and-adding-it-to-the-ssh-agent)
3. [Add SSH key to GitHub account](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/adding-a-new-ssh-key-to-your-github-account)

## Cloning Repositories

In Terminal:

1. Navigate to file location where you want to clone the repo to
1. In the online repo, click the green “Code” dropdown button. Copy the SSH link.
1. In terminal, run `git clone sshlinkhere`
1. For example, if you want to clone the `slonk` repo,

```sh
git clone git@github.com:rice-eclipse/slonk.git
```

## Naming convention

We’ll loosely follow 3 types of branches:

1. `feature -`a branch that works on implementing a specific feature, to be merged with `dev`
2. `bugfix -`a branch that works on fixing a specific bug, to be merged with `dev`(or another target branch)
3. `sandbox -`experimental branches that you probably will never merge with the “baseline” release. You can do whatever you want in this branch- it’s yours. Just don’t merge it with anything.

Every branch name should encode 2 descriptors:

1. branch type
2. dash-separated brief description of what the job is (that matches the job’s label on Trello board, if applicable)

We will use underscores to separate these 2 items. For example, the following are good branch names:

* `feature_big-blue-ignition-light`
* `bugfix_emergency-stop`
* `sandbox_learning-rust`

## Working with Git

* Cloning the repo downloads a copy of it to your local machine. Make sure you read this BEFORE you start pushing any changes.
* We will mostly be working off of the branch `dev`. There shouldn’t be any reason to be pushing ANY changes to `master`, unless you have a very very good reason to (which is very unlikely).

### Branches

To see list of all git branches:

```sh
git branch
```

Switch to the `dev` branch:

```sh
git checkout dev
```

Create new branch that you will be working on (follow above naming conventions):

```sh
git checkout -b branchname
```

After making any changes, check the status of your modified code:

```sh
git status
```

Add all modified files that you want to push to remote:

```sh
git add file1 file2 file3
```

Alternatively, to add all files in the working directory, do

```sh
git add .
```

To add all files not in gitignore, do

```sh
git add -A
```

Commit your changes, and add a description of what changes you made:

```sh
git commit -m "description here"
```

Commit messages are *very* important !!!! Make sure they accurately document what changes you made.

If your branch is newly created, set its upstream remote branch:

```sh
git push -u origin branchname
```

Push to remote repository:

```sh
git push
```

Congrats! You should now see your newly pushed changes to your branch in the remote repo on GitHub.

### Merging

Once you have finished working on your branch and would like to merge it with `dev` (or any other target branch):

1. Pull any changes that may have been made to `dev` to your local machine with `git pull`
1. Resolve any merge conflicts FIRST before moving forward.

Once you have resolved any merge conflicts, then you can move forward with merging your branch with `dev`:

1. Go to the online repo and click the “Pull Requests” tab
1. Click “New Pull Request”
1. Select the branch you want to merge with `dev`
1. Include any descriptions or notes about your branch that might be relevant
1. Click “Create Pull Request”
1. Wait for someone to check your code and approve your request (ideally.)
    1. We want `dev` to be a working version of our code at all times.
    1. Only approve the pull request if you’re *really really* sure you know what you’re doing and that it won’t break anything.
1. Once your request has been approved, congrats! Your code is now integrated into `dev`.
1. It would also be nice to notify the team that you have made changes to `dev` so we can update our local code accordingly.
