### Dependencies

This script needs the [biogl](https://github.com/glarue/biogl) module to function properly. If you use (or can get) `pip`, you can simply do

```python3 -m pip install biogl```

to add the package to a location reachable by your Python installation. 

Otherwise, you can clone the `biogl` repo and source it locally (to run from anywhere, you'll need to add it to your PYTHONPATH environmental variable, a process that varies by OS):

```git clone https://github.com/glarue/biogl.git```

### Usage info

```
usage: notify [-h] [-e EMAIL] [-o] [--add_email] [--view_config] [--ID ID]
              [-d]
              [external commands [external commands ...]]

Automatically sends an email to the specified address upon completion of the
specified command. Useful primarily for very long-running processes. In many
cases, the command being run (including the name of the other program) will
need to be placed in quotes. If no email address is provided with the -e flag,
a prompt will be displayed based upon the configuration file.

positional arguments:
  external commands     External commands to run, including external program
                        call. NOTE: for complex commands (e.g. awk + $args),
                        wrapping the entire command in double or triple quotes
                        may be necessary (default: None)

optional arguments:
  -h, --help            show this help message and exit
  -e EMAIL, --email EMAIL
                        the email address to notify (default: None)
  -o, --send_output     send any stdout/stderr messages in the body of the
                        email (limited to 5 MB) (default: False)
  --add_email           add or change an email address in the config file
                        (default: False)
  --view_config         view the contents of the configuration file (default:
                        False)
  --ID ID               additional string to include in email subject
                        (default: None)
  -d, --dry_run         print command that would be executed and exit without
                        running (default: False)
```

## __[tl;dr]__
`notify` will run any command, wait for it to complete, and send an email to the user once the command is finished. Particularly useful for long-running programs when run in a `screen`

## __[details]__
When running very long programs, it's nice to not have to think to check on them until they're finished. Also, having an email history of commands run can be a useful reference for future work. `notify` provides an accessible mechanism for keeping track of long-running processes and produces a de facto archive of previous commands.

`notify` can store information about the email server in a configuration file - this will be presented as an option to the user automatically. In addition, it can store information about users, to avoid the user having to enter their email address every time the script is run (though this can be avoided in a variety of other ways, e.g. through aliasing). User information may also be specified on a per-run basis (see usage info).

## __[example usage]__
One requirement of `notify` is that the command being run must be wrapped in quotes – while not required for all commands, failing to use quotes risks breaking the function of the script.

Here is an example using `samtools`,

```
$ notify -e user@email.com "samtools view -b tfoetus.stringtie.sam | samtools sort -o tfoetus.stringtie.sorted.bam"
```
which results in the following email send to 'user@email.com':

subject: 
```
[2017.07.05-13.50][server]: 'samtools view -b tfoetus.stringtie.sam | samtools sort -o tfoetus.stringtie.sorted.bam' completed
```
body:
```
Command-line arguments: samtools view -b tfoetus.stringtie.sam | samtools sort -o tfoetus.stringtie.sorted.bam
Total runtime: 2.704 hours
Return value: 0
Location: /mnt/server_drive/glarue/u12/protists/tritrichomonas_foetus
```
