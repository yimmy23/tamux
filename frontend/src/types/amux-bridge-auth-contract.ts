type Assert<T extends true> = T;

type HasKey<T, K extends PropertyKey> = K extends keyof T ? true : false;

type OpenAICodexAuthStatusResult = Awaited<
  ReturnType<NonNullable<AmuxBridge["openAICodexAuthStatus"]>>
>;

type ExpectedOpenAICodexAuthStatusResult = {
  available: boolean;
  authMode?: string;
  accountId?: string;
  expiresAt?: number;
  source?: string;
  error?: string;
};

type OpenAICodexAuthLoginResult = Awaited<
  ReturnType<NonNullable<AmuxBridge["openAICodexAuthLogin"]>>
>;

type ExpectedOpenAICodexAuthLoginResult = {
  available: boolean;
  authMode?: string;
  accountId?: string;
  expiresAt?: number;
  source?: string;
  error?: string;
  authUrl?: string;
};

export type OpenAICodexAuthStatusShapeIsCompatible = Assert<
  OpenAICodexAuthStatusResult extends ExpectedOpenAICodexAuthStatusResult ? true : false
>;

export type OpenAICodexAuthStatusOmitsApiKey = Assert<
  HasKey<OpenAICodexAuthStatusResult, "api_key"> extends false ? true : false
>;

export type OpenAICodexAuthLoginShapeIsCompatible = Assert<
  OpenAICodexAuthLoginResult extends ExpectedOpenAICodexAuthLoginResult ? true : false
>;

export type OpenAICodexAuthLoginOmitsApiKey = Assert<
  HasKey<OpenAICodexAuthLoginResult, "api_key"> extends false ? true : false
>;

export type OpenAICodexAuthLoginIncludesAuthUrl = Assert<
  HasKey<OpenAICodexAuthLoginResult, "authUrl">
>;

export {};
