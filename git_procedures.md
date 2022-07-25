# Standard Git Procedures

The codebase for all things software for Rice Eclipse is on GitHub ([https://github.com/rice-eclipse](https://github.com/rice-eclipse)). This document contains all relevant procedures and information on how to appropriately use Git version control for our software development.

We will also be using GitHub Issues to keep track of specific tasks that we come up with.


# Git Installation and Setup



1. [Install Git](https://git-scm.com/downloads)
2. [Configure username](https://docs.github.com/en/get-started/getting-started-with-git/setting-your-username-in-git)
3. [Configure commit email address](https://docs.github.com/en/account-and-profile/setting-up-and-managing-your-personal-account-on-github/managing-email-preferences/setting-your-commit-email-address#setting-your-commit-email-address-in-git)


# Setting up SSH key (if you haven’t already)

Use the Git Bash terminal application. Follow these instructions:



1. [Check for existing SSH keys](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/checking-for-existing-ssh-keys): 
2. [Generate new SSH key and add to SSH agent](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/generating-a-new-ssh-key-and-adding-it-to-the-ssh-agent)
3. [Add SSH key to GitHub account](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/adding-a-new-ssh-key-to-your-github-account)


# Cloning Repositories

In Terminal:



1. Navigate to file location where you want to clone the repo to
2. In the online repo, click the green “Code” dropdown button. Copy the SSH link. 
3. In terminal,

	`$ git clone sshlinkhere`



4. For example, if you want to clone the RESFET 2 repo,

    ```
    $ git clone git@github.com:rice-eclipse/resfet-controller-2.git

    ```



# Naming convention



* We’ll loosely follow 3 types of branches:
1. `feature - `a branch that works on implementing a specific feature, to be merged with `dev`
2. `bugfix - `a branch that works on fixing a specific bug, to be merged with `dev `(or another target branch)
3. `sandbox - `experimental branches that you probably will never merge with the “baseline” release. You can do whatever you want in this branch- it’s yours. Just don’t merge it with anything. 
* Every branch name should encode 2 descriptors: 
1. branch type
2. dash-separated brief description of what the job is (that matches the job’s label on Trello board, if applicable)
* We will use underscores to separate these 2 items. For example, the following are good branch names:
    * `feature_big-blue-ignition-light`
    * `bugfix_emergency-stop`
    * `sandbox_learning-rust`


# Working with Git



* Cloning the repo downloads a copy of it to your local machine. Make sure you read this BEFORE you start pushing any changes.
* We will mostly be working off of the branch `dev`. There shouldn’t be any reason to be pushing ANY changes to `master`, unless you have a very very good reason to (which is very unlikely). 


## Branches



1. To see list of all git branches:

    	`$ git branch`

2. Switch to the `dev` branch: 

        ```
        $ git checkout dev
        ```


3. Create new branch that you will be working on (follow above naming conventions): 

		`$ git checkout -b branchname`



4. After making any changes, check the status of your modified code:

		`$ git status`



5. Add all modified files that you want to push to remote:

		`$ git add file1 file2 file3`

	Alternatively, to add all files in the working directory, do


```
		$ git add .
```


To add all files not in gitignore, do


```
		$ git add -A

```



6. Commit your changes, and add a description of what changes you made:

    	`$ git commit -m "description here"`


    Commit messages are <span style="text-decoration:underline;">very</span> important !!!! Make sure they accurately document what changes you made. 

7. If your branch is newly created, set its upstream remote branch:

    	`$ git push -u origin branchname`

8. Push to remote repository:

    	`$ git push `

9. Congrats! You should now see your newly pushed changes to your branch in the remote repo on GitHub.


## Merging



* Once you have finished working on your branch and would like to merge it with `dev` (or any other target branch):
1. Pull any changes that may have been made to `dev` to your local machine:

        ```
        $ git pull
        ```


2. Resolve any merge conflicts FIRST before moving forward. 
* Once you have resolved any merge conflicts, then you can move forward with merging your branch with `dev:`
1. Go to the online repo and click the “Pull Requests” tab
2. Click “New Pull Request”
3. Select the branch you want to merge with `dev`
4. Include any descriptions or notes about your branch that might be relevant
5. Click “Create Pull Request”
6. Wait for someone to check your code and approve your request (ideally.)
    1. We want `dev` to be a working version of our code at all times. 
    2. Only approve the pull request if you’re _really <span style="text-decoration:underline;">really</span> _sure you know what you’re doing and that it won’t break anything. 
7. Once your request has been approved, congrats! Your code is now integrated into `dev`. 
8. It would also be nice to notify the team that you have made changes to `dev` so we can update our local code accordingly. 