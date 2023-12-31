-- Add down migration script here
create extension if not exists "uuid-ossp";

create table "user"
(
    id int not null,
    user_uuid  uuid not null default(uuid_generate_v4()),
    first_name text not null,
    last_name text null,
    date_of_birth date not null,
    created_at timestamptz(6) not null default now()
);

alter table "user" alter column id add generated by default as identity;
alter table "user" add constraint pk_user_id primary key(id);
alter table "user" add constraint ix_user_user_uuid unique(user_uuid);

insert into "user" (first_name, date_of_birth)
values ('DEFAULT', current_date), ('GUEST', current_date), ('SYSTEM', current_date);