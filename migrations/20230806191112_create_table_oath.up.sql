-- Add up migration script here
create table oath
(
	id              int            not null,
	target          text           not null,
	penalty         text           not null,
	starts_at       timestamptz(3) not null,
	ends_at         timestamptz(3) not null,
	ends_at_alias   text           not null,
	user_id         int            not null,
	created_at      timestamptz(6) not null default now(),
	last_updated_at timestamptz(6) not null default now()
);

alter table oath alter column id add generated by default as identity;
alter table oath add constraint pk_oath_id primary key(id);
alter table oath add constraint fk_oath_user foreign key (user_id) references "user" (id);