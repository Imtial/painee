-- Add down migration script here
ALTER TABLE oath
DROP CONSTRAINT pk_oath_id;

DROP TABLE oath;