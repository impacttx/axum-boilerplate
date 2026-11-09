CREATE TYPE public.device_platform_enum AS ENUM (
    'IOS',
    'ANDROID'
);

CREATE TABLE public.users (
    id uuid NOT NULL,
    email character varying,
    mobile_number character varying,
    profile_picture character varying,
    first_name character varying,
    last_name character varying,
    bio character varying,
    hashed_password character varying,
    is_active boolean DEFAULT false NOT NULL,
    is_admin boolean DEFAULT false NOT NULL,
    last_login timestamp with time zone,
    date_of_birth date,
    accepted_terms jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone,
    last_activity timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT users_pkey PRIMARY KEY (id),
    CONSTRAINT users_email_key UNIQUE (email),
    CONSTRAINT users_mobile_number_key UNIQUE (mobile_number)
);

CREATE TABLE public.roles (
    id uuid NOT NULL,
    name character varying(32) NOT NULL,
    description character varying,
    is_active boolean DEFAULT true NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT roles_pkey PRIMARY KEY (id),
    CONSTRAINT roles_name_key UNIQUE (name)
);

CREATE TABLE public.user_role_assignments (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    role_id uuid NOT NULL,
    assigned_by uuid,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT user_role_assignments_pkey PRIMARY KEY (id),
    CONSTRAINT uq_user_role UNIQUE (user_id, role_id),
    CONSTRAINT fk_user_role_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE,
    CONSTRAINT fk_user_role_role FOREIGN KEY (role_id) REFERENCES public.roles(id) ON DELETE CASCADE,
    CONSTRAINT fk_user_role_assigned_by FOREIGN KEY (assigned_by) REFERENCES public.users(id) ON DELETE SET NULL
);

CREATE TABLE public.jwt_blacklist (
    jti character varying NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT jwt_blacklist_pkey PRIMARY KEY (jti)
);

CREATE TABLE public.user_devices (
    device_id character varying NOT NULL,
    user_id uuid NOT NULL,
    platform public.device_platform_enum,
    device_name character varying,
    expo_push_token character varying,
    voip_push_token character varying,
    fcm_token character varying,
    is_active boolean DEFAULT true NOT NULL,
    last_active_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone,
    CONSTRAINT user_devices_pkey PRIMARY KEY (device_id),
    CONSTRAINT uq_user_device UNIQUE (user_id, device_id),
    CONSTRAINT fk_user_device_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE
);

CREATE TABLE public.mobile_otp_records (
    mobile_number character varying NOT NULL,
    hashed_otp character varying NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT mobile_otp_records_pkey PRIMARY KEY (mobile_number)
);

CREATE TABLE public.two_factor_codes (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    code character varying NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone,
    CONSTRAINT two_factor_codes_pkey PRIMARY KEY (id),
    CONSTRAINT fk_two_factor_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE
);

CREATE TABLE public.items (
    id uuid NOT NULL,
    title character varying(255) NOT NULL,
    description text,
    is_completed boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone,
    CONSTRAINT items_pkey PRIMARY KEY (id)
);
