# src/main.rs

RULE
:: "you can always optimize code you understand, you can't debug code you don't"

Note
:: lines() is one allocation for string and one drop when last_line is overwritten

Note
:: last_line takes the ownership (copy pointer,length and capacity of allocated string)

