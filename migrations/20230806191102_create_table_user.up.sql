-- Add down migration script here
create table "user"
(
    id int not null,
    first_name text not null,
    last_name text null,
    date_of_birth date not null,
    created_at timestamptz(6) not null default now()
);

alter table "user" alter column id add generated by default as identity;
alter table "user" add constraint pk_user_id primary key(id);

insert into "user" (first_name, date_of_birth)
values ('DEFAULT', current_date), ('GUEST', current_date), ('SYSTEM', current_date);